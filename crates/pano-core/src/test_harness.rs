//! 适配器一致性测试基座（AGENTS §3 / spec §8）。
//!
//! 每个适配器必须通过以下三项检查：
//! 1. **生命周期**：start → 产出样本 → stop 干净退出（stop 后不再产出）；
//! 2. **采样周期**：样本平均间隔在容差内（0.5x ~ 2x 期望周期）；
//! 3. **非法配置**：返回 [`AdapterError::Config`]，且失败后状态不是 `Running`。
//!
//! 用法（在各适配器测试模块中）：
//! ```ignore
//! #[test]
//! fn conformance() {
//!     pano_core::test_harness::run_all(
//!         &mut adapter,
//!         &[SeriesId::new(&adapter.meta().id, "value")], // 主 series（适配器必推指标）
//!         valid_config, sampling, invalid_config,
//!     )
//!     .expect("一致性测试失败");
//! }
//! ```
//!
//! `series` 传入本适配器**必推**的一个或多个指标（如 `sys.cpu` 传 `usage`）；
//! 采样周期与 stop 后不再产出检查以第一个 series 为准（多 series 适配器只要
//! 主指标满足即可，可选指标如 `per_core` 关闭时不产出属正常）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterStatus, ConfigValue, SampleSink, SeriesId,
};
use crate::sample_store::SampleStore;

/// 运行全部一致性检查；返回 `Err` 时携带失败详情（供测试断言）。
pub fn run_all(
    adapter: &mut dyn Adapter,
    series: &[SeriesId],
    valid_config: HashMap<String, ConfigValue>,
    sampling: Duration,
    invalid_config: HashMap<String, ConfigValue>,
) -> Result<(), String> {
    let primary = series
        .first()
        .ok_or_else(|| "需至少传入一个待检查的 series（适配器必推指标）".to_string())?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建 tokio runtime 失败：{e}"))?;
    let store = Arc::new(SampleStore::with_capacity(64));

    // 1. 生命周期 + 2. 采样周期
    let sink = SampleSink::new({
        let store = Arc::clone(&store);
        move |series, sample| store.push(series, sample)
    });
    let ctx = AdapterContext {
        sink,
        sampling,
        runtime: rt.handle().clone(),
        config: valid_config,
        http: None,
    };
    adapter
        .start(ctx)
        .map_err(|e| format!("start 失败（生命周期检查）：{e}"))?;
    if !matches!(adapter.status(), AdapterStatus::Running) {
        return Err(format!(
            "start 后状态应为 Running，实际：{}",
            adapter.status()
        ));
    }

    // 等待样本积累：轮询直到主 series 达到 3 个样本，或超时退出。
    // 适配器可能有冷启动延迟（如 sysinfo 首次刷新约 1s，见 roadmap §M2），
    // 故用轮询而非固定 sleep(sampling * 5)，避免首样本延迟导致误判。
    let deadline = std::time::Instant::now()
        + std::cmp::max(sampling.saturating_mul(8), Duration::from_secs(3));
    loop {
        if store.history(primary, usize::MAX).len() >= 3 {
            break;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let samples = store.history(primary, usize::MAX);
    if samples.len() < 3 {
        return Err(format!(
            "样本数量不足：期望 >= 3，实际 {}（采样周期检查）",
            samples.len()
        ));
    }
    let mut total = Duration::ZERO;
    for pair in samples.windows(2) {
        total += pair[1]
            .timestamp
            .duration_since(pair[0].timestamp)
            .unwrap_or_default();
    }
    let avg = total / (samples.len() as u32 - 1);
    if avg < sampling / 2 || avg > sampling * 2 {
        return Err(format!(
            "采样周期偏差过大：期望约 {sampling:?}，实际平均 {avg:?}"
        ));
    }

    // stop 必须干净退出
    adapter
        .stop()
        .map_err(|e| format!("stop 失败（生命周期检查）：{e}"))?;
    if matches!(
        adapter.status(),
        AdapterStatus::Running | AdapterStatus::Starting
    ) {
        return Err(format!("stop 后状态异常：{}", adapter.status()));
    }
    let before = store.history(primary, usize::MAX).len();
    std::thread::sleep(sampling * 2);
    let after = store.history(primary, usize::MAX).len();
    if after != before {
        return Err(format!(
            "stop 后仍在产出样本（生命周期检查）：{before} -> {after}"
        ));
    }

    // 3. 非法配置
    let sink2 = SampleSink::new(|_series, _sample| {});
    let ctx2 = AdapterContext {
        sink: sink2,
        sampling,
        runtime: rt.handle().clone(),
        config: invalid_config,
        http: None,
    };
    match adapter.start(ctx2) {
        Err(AdapterError::Config(_)) => {
            if matches!(adapter.status(), AdapterStatus::Running) {
                return Err("非法配置失败后状态不应为 Running".to_string());
            }
            Ok(())
        }
        Err(e) => Err(format!("非法配置应返回 AdapterError::Config，实际：{e:?}")),
        Ok(()) => Err("非法配置不应启动成功".to_string()),
    }
}
