//! 远程适配器：`deepseek.balance` —— DeepSeek API 额度监控（M2.5）。
//!
//! - 数据源：`GET {base_url}/user/balance`（`Authorization: Bearer <api_key>`，
//!   DeepSeek 官方余额查询端点）。响应：
//!   `{ is_available, balance_infos: [{ currency, total_balance, granted_balance,
//!   topped_up_balance }] }`——金额为**字符串**（如 `"110.00"`），数组按币种分项
//!   （CNY / USD），`total = granted(赠送) + topped_up(充值)`。
//! - series（按响应中实际出现的币种产出；未知币种 warn 跳过）：
//!   - `deepseek.balance.total_cny` / `granted_cny` / `topped_up_cny`（Number，¥）；
//!   - `deepseek.balance.total_usd` / `granted_usd` / `topped_up_usd`（Number，$）；
//!   - `deepseek.balance.is_available`（Bool，账户可用状态）。
//! - 自定义配置（`config_schema`）：
//!   - `api_key`（Text，可选填）——DeepSeek API 密钥；**环境变量
//!     `PANO_DEEPSEEK_API_KEY` 优先**，回落本字段；两者皆无 → `AdapterError::Config`；
//!   - `base_url`（Text，默认 `https://api.deepseek.com`）——服务基址（测试可指向本地）；
//!   - `low_threshold`（Number，默认 20，域 0..=10000）——低余额阈值（¥），仅作
//!     数据源配置暴露给 UI 指示灯判定（适配器自身不使用，仿 `high_threshold` 模式）。
//! - 能力：`TimeSeries` + `RemoteSource`（R3 标记，管理页显示「远程数据源」标签）。
//!
//! ## R3 注入（架构 §14，M2.5 首次落地）
//!
//! 本适配器不直接依赖 reqwest：HTTP 客户端由 pano-app 在 feature
//! `adapter-deepseek-balance` 下构造 [`crate::remote::http_poll::ReqwestHttpClient`]
//! 并经 `Lifecycle::with_http` 注入（`AdapterContext.http`）；`start` 中为 `None` →
//! [`AdapterError::NotAvailable`]。轮询复用 [`crate::remote::http_poll::poll_loop`]
//! （core 级退避重试 + Error 阈值，架构 §9；连续失败达阈值 → `Error`，成功自动恢复）。
//!
//! ## 测试（远程型一致性例外，spec §10 / roadmap M4）
//!
//! `pano_core::test_harness::run_all` 固定注入 `http: None` 不适用 → 本模块内置
//! **loopback 假 DeepSeek 服务**（`std::net::TcpListener` 线程响应固定 JSON，零新增
//! 依赖），注入 `ReqwestHttpClient` 跑一致性流程：start → 样本到达 → 采样周期容差 →
//! stop 后无样本 → 非法配置拒绝（用 `low_threshold` 越界作确定性非法配置，不依赖环境变量）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleSink, SampleValue, SeriesId,
};
use pano_core::capability::Capability;
use pano_core::http::HttpResponse;

use crate::remote::http_poll::poll_loop;

const ID: &str = "deepseek.balance";
const ENDPOINT_USER_BALANCE: &str = "/user/balance";
const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";

const KEY_API_KEY: &str = "api_key";
const KEY_BASE_URL: &str = "base_url";
const KEY_LOW_THRESHOLD: &str = "low_threshold";
const DEFAULT_LOW_THRESHOLD: f64 = 20.0;
const LOW_THRESHOLD_MAX: f64 = 10_000.0;

/// api_key 环境变量（优先级高于配置段 `api_key` 字段）。
const ENV_API_KEY: &str = "PANO_DEEPSEEK_API_KEY";

const METRIC_IS_AVAILABLE: &str = "is_available";
const METRIC_TOTAL: &str = "total";
const METRIC_GRANTED: &str = "granted";
const METRIC_TOPPED_UP: &str = "topped_up";

/// 关注币种（`series()` 静态声明；响应中出现的币种才产出样本，其余 warn 跳过）。
const CURRENCIES: [&str; 2] = ["CNY", "USD"];

/// `deepseek.balance` 适配器：按采样周期轮询 DeepSeek 余额端点并推送样本。
pub struct DeepSeekBalance {
    /// 共享状态：轮询任务经 `poll_loop` 的 `on_status` 回调更新（Running / Error）。
    status: Arc<Mutex<AdapterStatus>>,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl DeepSeekBalance {
    /// 构造额度适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(AdapterStatus::Stopped)),
            task: None,
            runtime: None,
        }
    }

    /// api_key 解析（纯函数，env 注入便于测试）：`PANO_DEEPSEEK_API_KEY` 优先，
    /// 回落配置段 `api_key`；两者皆无 / 为空 → [`AdapterError::Config`]。
    fn resolve_api_key_from(
        config: &HashMap<String, ConfigValue>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<String, AdapterError> {
        if let Some(key) = env(ENV_API_KEY)
            && !key.trim().is_empty()
        {
            return Ok(key);
        }
        match config.get(KEY_API_KEY) {
            Some(ConfigValue::Text(s)) if !s.trim().is_empty() => Ok(s.clone()),
            Some(ConfigValue::Text(_)) => Err(AdapterError::Config("api_key 不能为空".into())),
            Some(other) => Err(AdapterError::Config(format!(
                "api_key 必须为文本（DeepSeek API 密钥），实际 {other:?}"
            ))),
            None => Err(AdapterError::Config(format!(
                "缺少 api_key：配置字段 {KEY_API_KEY} 与环境变量 {ENV_API_KEY} 均未提供"
            ))),
        }
    }

    /// 经真实环境变量解析 api_key。
    fn resolve_api_key(config: &HashMap<String, ConfigValue>) -> Result<String, AdapterError> {
        Self::resolve_api_key_from(config, &|k| std::env::var(k).ok())
    }

    /// 解析 `base_url`（服务基址）；缺省官方端点，空文本 → [`AdapterError::Config`]。
    fn resolve_base_url(config: &HashMap<String, ConfigValue>) -> Result<String, AdapterError> {
        match config.get(KEY_BASE_URL) {
            None => Ok(DEFAULT_BASE_URL.to_string()),
            Some(ConfigValue::Text(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            Some(ConfigValue::Text(_)) => Err(AdapterError::Config("base_url 不能为空".into())),
            Some(other) => Err(AdapterError::Config(format!(
                "base_url 必须为文本，实际 {other:?}"
            ))),
        }
    }

    /// 解析 `low_threshold`（低余额阈值，¥，0..=10000）；缺省 20，非法 → Config。
    fn resolve_low_threshold(config: &HashMap<String, ConfigValue>) -> Result<f64, AdapterError> {
        match config.get(KEY_LOW_THRESHOLD) {
            None => Ok(DEFAULT_LOW_THRESHOLD),
            Some(ConfigValue::Number(n))
                if n.is_finite() && (0.0..=LOW_THRESHOLD_MAX).contains(n) =>
            {
                Ok(*n)
            }
            Some(other) => Err(AdapterError::Config(format!(
                "low_threshold 必须为 0..={LOW_THRESHOLD_MAX} 的数值，实际 {other:?}"
            ))),
        }
    }
}

impl Default for DeepSeekBalance {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for DeepSeekBalance {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "DeepSeek 额度".to_string(),
            description:
                "DeepSeek API 余额监控（total / granted / topped_up，CNY / USD，账户可用状态）"
                    .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::new(Capability::TIME_SERIES),
            Capability::new(Capability::REMOTE_SOURCE),
        ]
    }

    fn series(&self) -> Vec<SeriesId> {
        let id = self.meta().id;
        let mut out = vec![SeriesId::new(&id, METRIC_IS_AVAILABLE)];
        for currency in CURRENCIES {
            let c = currency.to_ascii_lowercase();
            out.push(SeriesId::new(&id, &format!("{METRIC_TOTAL}_{c}")));
            out.push(SeriesId::new(&id, &format!("{METRIC_GRANTED}_{c}")));
            out.push(SeriesId::new(&id, &format!("{METRIC_TOPPED_UP}_{c}")));
        }
        out
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![
                ConfigField {
                    key: KEY_API_KEY.to_string(),
                    label: "API 密钥".to_string(),
                    kind: FieldKind::Text,
                    default: ConfigValue::Text(String::new()),
                    help: Some(format!(
                        "DeepSeek API 密钥；也可经环境变量 {ENV_API_KEY} 提供（优先级更高）"
                    )),
                },
                ConfigField {
                    key: KEY_BASE_URL.to_string(),
                    label: "服务地址".to_string(),
                    kind: FieldKind::Text,
                    default: ConfigValue::Text(DEFAULT_BASE_URL.to_string()),
                    help: Some("API 服务基址（默认官方端点；测试可指向本地 mock）".to_string()),
                },
                ConfigField {
                    key: KEY_LOW_THRESHOLD.to_string(),
                    label: "低余额阈值".to_string(),
                    kind: FieldKind::Number,
                    default: ConfigValue::Number(DEFAULT_LOW_THRESHOLD),
                    help: Some(format!(
                        "CNY 总余额低于该值（¥，0..={LOW_THRESHOLD_MAX}）时，额度窗口指示灯亮红"
                    )),
                },
            ],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        // 配置校验（非法配置在启动即拒绝，一致性测试要求）
        let api_key = Self::resolve_api_key(&ctx.config)?;
        let base_url = Self::resolve_base_url(&ctx.config)?;
        Self::resolve_low_threshold(&ctx.config)?;
        let http = ctx.http.clone().ok_or_else(|| {
            AdapterError::NotAvailable(
                "未注入 HttpClient（需启用 adapter-deepseek-balance feature 装配）".into(),
            )
        })?;

        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = AdapterStatus::Running;
        self.runtime = Some(ctx.runtime.clone());

        let status = Arc::clone(&self.status);
        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let adapter_id = self.meta().id;
        let url = format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            ENDPOINT_USER_BALANCE
        );
        let headers = vec![("Authorization".to_string(), format!("Bearer {api_key}"))];
        let task_url = url.clone(); // 移入任务闭包；外层保留原始串供启动日志

        self.task = Some(ctx.runtime.spawn(async move {
            poll_loop(
                http.as_ref(),
                &task_url,
                &headers,
                sampling,
                move |response: HttpResponse| push_samples(&adapter_id, &sink, &response),
                move |new_status| {
                    *status.lock().unwrap_or_else(|e| e.into_inner()) = new_status;
                },
            )
            .await;
        }));

        tracing::info!(target: "pano::adapters::deepseek_balance", url = %url, "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        // 上下文安全的 join：非运行时线程阻塞等待；运行时内（async 命令路径）
        // 仅 abort（避免「Cannot start a runtime from within a runtime」panic），
        // 见 util::shutdown_task / roadmap §M2.1。
        crate::util::shutdown_task(self.task.take(), self.runtime.as_ref());
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::deepseek_balance", "适配器已停止");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// 解析余额响应并推送样本（一次轮询回调）。
///
/// 金额字段为字符串（如 `"110.00"`）；金额解析失败 / 未知币种 → warn 跳过
/// （不崩溃、不产出脏数据）；`is_available` 恒推送。
fn push_samples(
    adapter_id: &pano_core::adapter::AdapterId,
    sink: &SampleSink,
    response: &HttpResponse,
) {
    let timestamp = pano_core::adapter::now();
    let balance = match parse_balance(&response.body) {
        Ok(balance) => balance,
        Err(error) => {
            tracing::warn!(
                target: "pano::adapters::deepseek_balance",
                error = %error,
                "解析余额响应失败（本次轮询跳过）"
            );
            return;
        }
    };

    sink.push(
        SeriesId::new(adapter_id, METRIC_IS_AVAILABLE),
        Sample {
            timestamp,
            value: SampleValue::Bool(balance.is_available),
        },
    );

    for info in &balance.balance_infos {
        let Ok(currency) = Currency::parse(&info.currency) else {
            tracing::warn!(
                target: "pano::adapters::deepseek_balance",
                currency = %info.currency,
                "未知币种，跳过该余额分项"
            );
            continue;
        };
        let c = currency.suffix();
        push_amount(
            adapter_id,
            sink,
            &format!("{METRIC_TOTAL}_{c}"),
            info.total_balance.as_deref(),
        );
        push_amount(
            adapter_id,
            sink,
            &format!("{METRIC_GRANTED}_{c}"),
            info.granted_balance.as_deref(),
        );
        push_amount(
            adapter_id,
            sink,
            &format!("{METRIC_TOPPED_UP}_{c}"),
            info.topped_up_balance.as_deref(),
        );
    }
}

/// 推送单个金额 series（字符串金额 → f64；缺省 / 解析失败 warn 跳过）。
fn push_amount(
    adapter_id: &pano_core::adapter::AdapterId,
    sink: &SampleSink,
    metric: &str,
    amount: Option<&str>,
) {
    let timestamp = pano_core::adapter::now();
    let Some(amount) = amount else {
        tracing::warn!(target: "pano::adapters::deepseek_balance", series = %metric, "余额字段缺省，跳过");
        return;
    };
    let Some(value) = parse_amount(amount) else {
        tracing::warn!(target: "pano::adapters::deepseek_balance", series = %metric, amount, "金额解析失败，跳过");
        return;
    };
    sink.push(
        SeriesId::new(adapter_id, metric),
        Sample {
            timestamp,
            value: SampleValue::Number(value),
        },
    );
}

/// 余额响应（DeepSeek `GET /user/balance`）。
#[derive(Debug, serde::Deserialize)]
struct BalanceResponse {
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct BalanceInfo {
    currency: String,
    /// 金额为字符串（如 `"110.00"`）；字段缺省 / 为 null 按 `Option` 容错。
    #[serde(default)]
    total_balance: Option<String>,
    #[serde(default)]
    granted_balance: Option<String>,
    #[serde(default)]
    topped_up_balance: Option<String>,
}

/// 关注币种（当前支持 CNY / USD；其余未知币种跳过）。
enum Currency {
    Cny,
    Usd,
}

impl Currency {
    fn parse(code: &str) -> Result<Self, ()> {
        match code {
            "CNY" => Ok(Self::Cny),
            "USD" => Ok(Self::Usd),
            _ => Err(()),
        }
    }

    /// series 后缀（小写，如 `cny`）。
    fn suffix(&self) -> &'static str {
        match self {
            Self::Cny => "cny",
            Self::Usd => "usd",
        }
    }
}

/// 解析余额响应体（serde_json）；失败返回描述性错误（供日志）。
fn parse_balance(body: &[u8]) -> Result<BalanceResponse, String> {
    serde_json::from_slice(body).map_err(|e| format!("余额响应解析失败：{e}"))
}

/// 字符串金额 → f64（如 `"110.00"` → 110.0）；空 / 非法 → `None`。
fn parse_amount(s: &str) -> Option<f64> {
    let n = s.trim().parse::<f64>().ok()?;
    n.is_finite().then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pano_core::adapter::{
        AdapterContext, AdapterError, AdapterStatus, ConfigValue, SampleSink,
    };
    use pano_core::sample_store::SampleStore;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use crate::remote::http_poll::ReqwestHttpClient;

    /// 假 DeepSeek 服务固定返回的余额响应（CNY + USD 两个分项）。
    const MOCK_BODY: &str = r#"{"is_available":true,"balance_infos":[
        {"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"},
        {"currency":"USD","total_balance":"15.25","granted_balance":"5.25","topped_up_balance":"10.00"}
    ]}"#;

    // ---------- 配置解析单元测试 ----------

    #[test]
    fn resolve_api_key_env_priority_and_fallback() {
        let empty = HashMap::new();
        // env 提供 → 优先（即使配置段也有）
        let mut cfg = HashMap::new();
        cfg.insert(KEY_API_KEY.to_string(), ConfigValue::Text("cfg-key".into()));
        assert_eq!(
            DeepSeekBalance::resolve_api_key_from(&cfg, &|k| (k == ENV_API_KEY)
                .then(|| "env-key".to_string()))
            .unwrap(),
            "env-key"
        );
        // env 未提供 → 回落配置段
        assert_eq!(
            DeepSeekBalance::resolve_api_key_from(&cfg, &|_| None).unwrap(),
            "cfg-key"
        );
        // 两者皆无 → Config
        assert!(matches!(
            DeepSeekBalance::resolve_api_key_from(&empty, &|_| None),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn resolve_api_key_rejects_empty_and_wrong_type() {
        // 空文本
        let mut cfg = HashMap::new();
        cfg.insert(KEY_API_KEY.to_string(), ConfigValue::Text(String::new()));
        assert!(matches!(
            DeepSeekBalance::resolve_api_key_from(&cfg, &|_| None),
            Err(AdapterError::Config(_))
        ));
        // 非文本
        let mut cfg = HashMap::new();
        cfg.insert(KEY_API_KEY.to_string(), ConfigValue::Number(1.0));
        assert!(matches!(
            DeepSeekBalance::resolve_api_key_from(&cfg, &|_| None),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn resolve_base_url_default_and_invalid() {
        let empty = HashMap::new();
        assert_eq!(
            DeepSeekBalance::resolve_base_url(&empty).unwrap(),
            DEFAULT_BASE_URL
        );
        let mut cfg = HashMap::new();
        cfg.insert(
            KEY_BASE_URL.to_string(),
            ConfigValue::Text("http://127.0.0.1:9000/".into()),
        );
        assert_eq!(
            DeepSeekBalance::resolve_base_url(&cfg).unwrap(),
            "http://127.0.0.1:9000/"
        );
        let mut cfg = HashMap::new();
        cfg.insert(KEY_BASE_URL.to_string(), ConfigValue::Text(String::new()));
        assert!(matches!(
            DeepSeekBalance::resolve_base_url(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn resolve_low_threshold_default_and_domain() {
        let empty = HashMap::new();
        assert_eq!(
            DeepSeekBalance::resolve_low_threshold(&empty).unwrap(),
            20.0
        );
        let mut cfg = HashMap::new();
        cfg.insert(KEY_LOW_THRESHOLD.to_string(), ConfigValue::Number(5.0));
        assert_eq!(DeepSeekBalance::resolve_low_threshold(&cfg).unwrap(), 5.0);
        // 越界（> 10000 / < 0 / 非有限）与非数值 → Config
        for bad in [10_001.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut cfg = HashMap::new();
            cfg.insert(KEY_LOW_THRESHOLD.to_string(), ConfigValue::Number(bad));
            assert!(matches!(
                DeepSeekBalance::resolve_low_threshold(&cfg),
                Err(AdapterError::Config(_))
            ));
        }
        let mut cfg = HashMap::new();
        cfg.insert(KEY_LOW_THRESHOLD.to_string(), ConfigValue::Text("x".into()));
        assert!(matches!(
            DeepSeekBalance::resolve_low_threshold(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    // ---------- 响应解析单元测试 ----------

    #[test]
    fn parse_balance_amounts_and_currencies() {
        let body = br#"{"is_available":true,"balance_infos":[
            {"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}
        ]}"#;
        let parsed = parse_balance(body).expect("解析成功");
        assert!(parsed.is_available);
        assert_eq!(parsed.balance_infos.len(), 1);
        assert_eq!(
            parse_amount(parsed.balance_infos[0].total_balance.as_deref().unwrap()).unwrap(),
            110.0
        );
        assert_eq!(
            parse_amount(parsed.balance_infos[0].granted_balance.as_deref().unwrap()).unwrap(),
            10.0
        );
        assert_eq!(
            parse_amount(
                parsed.balance_infos[0]
                    .topped_up_balance
                    .as_deref()
                    .unwrap()
            )
            .unwrap(),
            100.0
        );
    }

    #[test]
    fn parse_balance_tolerates_missing_fields_and_empty_list() {
        // 字段缺省 / null / balance_infos 为空数组都不得解析失败
        let body = br#"{"is_available":false,"balance_infos":[]}"#;
        let parsed = parse_balance(body).expect("空数组解析成功");
        assert!(!parsed.is_available);
        assert!(parsed.balance_infos.is_empty());

        let body =
            br#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":null}]}"#;
        let parsed = parse_balance(body).expect("null 金额容错");
        assert!(parsed.balance_infos[0].total_balance.is_none());
    }

    #[test]
    fn parse_balance_rejects_malformed_body() {
        assert!(parse_balance(br#"not-json"#).is_err());
    }

    #[test]
    fn parse_amount_handles_invalid_strings() {
        assert_eq!(parse_amount("12.50"), Some(12.5));
        assert_eq!(parse_amount(" 3 "), Some(3.0));
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount("NaN"), None);
    }

    #[test]
    fn currency_parse_and_suffix() {
        assert_eq!(Currency::parse("CNY").unwrap().suffix(), "cny");
        assert_eq!(Currency::parse("USD").unwrap().suffix(), "usd");
        assert!(Currency::parse("EUR").is_err());
    }

    // ---------- loopback 假 DeepSeek 服务（一致性测试用，零新增依赖） ----------

    /// 测试用 loopback 假 DeepSeek 服务：任何请求都返回固定余额 JSON。
    ///
    /// 非阻塞 accept + 轮询停靠标志，`Drop` 时优雅退出线程（不泄漏 / 不挂测试）。
    struct MockBalanceServer {
        addr: SocketAddr,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl MockBalanceServer {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("绑定 mock 端口失败");
            listener.set_nonblocking(true).expect("设置非阻塞失败");
            let addr = listener.local_addr().expect("读取 mock 端口失败");
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = Arc::clone(&stop);
            let thread = std::thread::spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // 读请求头（到 \r\n\r\n 或超时），随后回固定 JSON。
                            let mut header = Vec::new();
                            let mut buf = [0u8; 4096];
                            let deadline = Instant::now() + Duration::from_millis(2000);
                            loop {
                                if header.windows(4).any(|w| w == b"\r\n\r\n")
                                    || header.len() > 8192
                                    || Instant::now() > deadline
                                {
                                    break;
                                }
                                match stream.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => header.extend_from_slice(&buf[..n]),
                                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                        std::thread::sleep(Duration::from_millis(5));
                                    }
                                    Err(_) => break,
                                }
                            }
                            let body = MOCK_BODY.as_bytes();
                            let mut resp = Vec::new();
                            resp.extend_from_slice(
                                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ",
                            );
                            resp.extend_from_slice(body.len().to_string().as_bytes());
                            resp.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
                            resp.extend_from_slice(body);
                            let _ = stream.write_all(&resp);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                addr,
                stop,
                thread: Some(thread),
            }
        }

        fn url(&self) -> String {
            format!("http://{}", self.addr)
        }
    }

    impl Drop for MockBalanceServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    // ---------- 一致性测试（远程型例外，spec §10） ----------

    #[test]
    fn conformance_with_mock_server() {
        let server = MockBalanceServer::spawn();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("创建 tokio runtime 失败");
        let store = Arc::new(SampleStore::with_capacity(64));
        let sink = SampleSink::new({
            let store = Arc::clone(&store);
            move |series, sample| store.push(series, sample)
        });
        let http: Option<Arc<dyn pano_core::http::HttpClient>> = Some(Arc::new(
            ReqwestHttpClient::new().expect("构造 reqwest 客户端失败"),
        ));

        let mut adapter = DeepSeekBalance::new();
        let mut cfg = HashMap::new();
        cfg.insert(KEY_API_KEY.to_string(), ConfigValue::Text("sk-test".into()));
        cfg.insert(KEY_BASE_URL.to_string(), ConfigValue::Text(server.url()));
        let sampling = Duration::from_millis(200);
        let ctx = AdapterContext {
            sink,
            sampling,
            runtime: rt.handle().clone(),
            config: cfg,
            http,
        };
        adapter.start(ctx).expect("start 成功");

        // start 返回即视为已开始（与本地适配器语义一致）
        assert!(matches!(adapter.status(), AdapterStatus::Running));

        // 等待主 series 达 3 样本（轮询等待，吸收首次请求 / 冷启动延迟）
        let primary = SeriesId::new(&adapter.meta().id, "total_cny");
        let deadline =
            Instant::now() + std::cmp::max(sampling.saturating_mul(8), Duration::from_secs(3));
        loop {
            if store.history(&primary, usize::MAX).len() >= 3 {
                break;
            }
            assert!(Instant::now() < deadline, "等待样本超时（采样周期检查）");
            std::thread::sleep(Duration::from_millis(50));
        }

        // 采样周期：样本平均间隔在 0.5x ~ 2x 期望周期内
        let samples = store.history(&primary, usize::MAX);
        let mut total = Duration::ZERO;
        for pair in samples.windows(2) {
            total += pair[1]
                .timestamp
                .duration_since(pair[0].timestamp)
                .unwrap_or_default();
        }
        let avg = total / (samples.len() as u32 - 1);
        assert!(
            avg >= sampling / 2 && avg <= sampling * 2,
            "采样周期偏差过大：期望约 {sampling:?}，实际平均 {avg:?}"
        );

        // 币种分项与可用状态一并产出（CNY + USD）
        let cny_total = store.history(&SeriesId::new(&adapter.meta().id, "total_cny"), usize::MAX);
        let usd_total = store.history(&SeriesId::new(&adapter.meta().id, "total_usd"), usize::MAX);
        assert!(!cny_total.is_empty(), "应产出 total_cny 样本");
        assert!(!usd_total.is_empty(), "应产出 total_usd 样本");
        assert_eq!(
            store
                .history(
                    &SeriesId::new(&adapter.meta().id, "is_available"),
                    usize::MAX
                )
                .last()
                .map(|s| &s.value),
            Some(&pano_core::adapter::SampleValue::Bool(true))
        );

        // stop 干净退出，且此后不再产出样本
        adapter.stop().expect("stop 成功");
        assert!(!matches!(
            adapter.status(),
            AdapterStatus::Running | AdapterStatus::Starting
        ));
        let before = store.history(&primary, usize::MAX).len();
        std::thread::sleep(sampling * 2);
        assert_eq!(
            store.history(&primary, usize::MAX).len(),
            before,
            "stop 后仍在产出样本（生命周期检查）"
        );

        // 非法配置：low_threshold 越界 → Config（确定性，不依赖环境变量）
        let mut bad = HashMap::new();
        bad.insert(KEY_API_KEY.to_string(), ConfigValue::Text("sk-test".into()));
        bad.insert(KEY_BASE_URL.to_string(), ConfigValue::Text(server.url()));
        bad.insert(KEY_LOW_THRESHOLD.to_string(), ConfigValue::Number(10_001.0));
        let ctx2 = AdapterContext {
            sink: SampleSink::new(|_series, _sample| {}),
            sampling,
            runtime: rt.handle().clone(),
            config: bad,
            http: None,
        };
        match adapter.start(ctx2) {
            Err(AdapterError::Config(_)) => {}
            other => panic!("非法配置应返回 AdapterError::Config，实际 {other:?}"),
        }
        assert!(
            !matches!(adapter.status(), AdapterStatus::Running),
            "非法配置失败后状态不应为 Running"
        );
    }
}
