//! 磁盘忙碌时间采样（M2.3.1）——**Windows 先行**。
//!
//! 语义：`sys.disk.active_percent` = 当前**最忙磁盘**的真实忙碌时间 %（0..=100，
//! 非字节速率）；配套盘符经 `sys.disk.busiest_disk`（Text）输出。
//!
//! Windows 用 PDH `\PhysicalDisk(*)\% Disk Time` 逐盘采集（真实忙碌时间），取
//! 非 `_Total` 实例中的最大值；统计范围为系统全部物理磁盘（`device` 过滤不作用
//! 于忙碌时间）。PDH 计数器需两次采集（间隔时间）才能形成有效值：启动时采集
//! 基线，之后每个采样 tick 采集并读取格式化数组。
//!
//! 其他平台（Linux / macOS）暂未实现：`DiskBusy::new()` 返回错误 → 适配器不产出
//! active_percent / busiest_disk（仪表显示「—」）。

#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

/// 从 PDH 格式化数组项中挑出最忙盘：排除 `_Total` 聚合项，取忙碌 % 最大者。
/// 返回 `(盘符, 忙碌 %)`；空或无有效项返回 `None`。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn busiest_from_items(items: &[(String, f64)]) -> Option<(String, f64)> {
    items
        .iter()
        .filter(|(name, _)| !name.starts_with("_Total"))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(name, pct)| (name.clone(), *pct))
}

/// 从 PDH 实例名提取盘符（`"0 C:"` → `"C:"`、`"C:"` → `"C:"`）；
/// 提取不到（如网络盘 / 非盘符实例）则回退原实例名。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn drive_letter(instance: &str) -> String {
    instance
        .split_whitespace()
        .find(|tok| {
            tok.len() == 2 && tok.as_bytes()[1] == b':' && tok.as_bytes()[0].is_ascii_alphabetic()
        })
        .unwrap_or(instance)
        .to_string()
}

#[cfg(target_os = "windows")]
pub(crate) struct DiskBusy {
    query: windows_sys::Win32::System::Performance::PDH_HQUERY,
    counter: windows_sys::Win32::System::Performance::PDH_HCOUNTER,
    last: Option<(String, f64)>,
    last_collect: Instant,
    /// 运行期采集失败是否已 warn（只告警一次，避免日志刷屏）。
    warned: bool,
}

/// PDH 采集间隔下限：过短的间隔会导致 %DiskTime 计算无意义（复用上次结果）。
#[cfg(target_os = "windows")]
const MIN_COLLECT_INTERVAL: Duration = Duration::from_millis(200);

#[cfg(target_os = "windows")]
impl DiskBusy {
    /// 打开 PDH 查询、添加 `\PhysicalDisk(*)\% Disk Time` 通配计数器并采集基线。
    pub(crate) fn new() -> Result<Self, String> {
        use windows_sys::Win32::System::Performance::{
            PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
            PdhOpenQueryW,
        };
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        // 成功 = 0（ERROR_SUCCESS）；失败按错误码上报
        let status = unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) };
        if status != 0 {
            return Err(format!("PdhOpenQueryW 失败 status={status:#x}"));
        }
        let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
        // 通配路径（英文计数器名）：% Disk Time（磁盘忙碌时间百分比）
        let path: Vec<u16> = "\\PhysicalDisk(*)\\% Disk Time"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let status = unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) };
        if status != 0 {
            unsafe { PdhCloseQuery(query) };
            return Err(format!("PdhAddEnglishCounterW 失败 status={status:#x}"));
        }
        // 基线采集：%DiskTime 需两次采集间隔后才有有效格式化值
        unsafe {
            PdhCollectQueryData(query);
        }
        Ok(Self {
            query,
            counter,
            last: None,
            last_collect: Instant::now(),
            warned: false,
        })
    }

    /// 运行期采集失败：仅首次 warn（后续静默，避免日志刷屏）。
    fn warn_once(&mut self, detail: &str) {
        if !self.warned {
            self.warned = true;
            tracing::warn!(
                target: "pano::adapters::sys_disk",
                "磁盘忙碌时间采集失败：{detail}（后续失败静默）"
            );
        }
    }

    /// 采集一次：返回 `(最忙盘盘符, 忙碌 %)`；间隔过短 / 采集失败时复用上次结果。
    pub(crate) fn sample(&mut self) -> Option<(String, f64)> {
        use windows_sys::Win32::System::Performance::{
            PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA, PdhCollectQueryData,
            PdhGetFormattedCounterArrayW,
        };
        let now = Instant::now();
        if now.duration_since(self.last_collect) < MIN_COLLECT_INTERVAL {
            return self.last.clone();
        }
        self.last_collect = now;

        unsafe {
            let status = PdhCollectQueryData(self.query);
            if status != 0 {
                self.warn_once(&format!("PdhCollectQueryData status={status:#x}"));
                return self.last.clone();
            }
            // 第一次调用：探测缓冲区大小（返回 PDH_MORE_DATA，lpdwbuffersize 给出所需字节）
            let mut buf_size: u32 = 0;
            let mut item_count: u32 = 0;
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                std::ptr::null_mut(),
            );
            if status != PDH_MORE_DATA || buf_size == 0 {
                self.warn_once(&format!(
                    "PdhGetFormattedCounterArrayW 探测 status={status:#x} buf={buf_size}"
                ));
                return self.last.clone();
            }
            // 第二次调用：填充缓冲区。缓冲区必须 ≥ 所需字节数（buf_size 不一定是
            // 结构体大小的整数倍，用 div_ceil 上取整，避免 PDH 越界写入 → 堆损坏）。
            let count =
                (buf_size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
            let mut items: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = vec![Default::default(); count];
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                items.as_mut_ptr(),
            );
            if status != 0 {
                self.warn_once(&format!(
                    "PdhGetFormattedCounterArrayW 填充 status={status:#x}"
                ));
                return self.last.clone();
            }

            let mut collected: Vec<(String, f64)> = Vec::with_capacity(item_count as usize);
            for item in items.iter().take(item_count as usize) {
                // CStatus == 0（ERROR_SUCCESS）才有效
                if item.FmtValue.CStatus != 0 {
                    continue;
                }
                let raw = item.FmtValue.Anonymous.doubleValue;
                if !raw.is_finite() {
                    continue;
                }
                let instance = decode_wide(item.szName);
                if instance.is_empty() {
                    continue;
                }
                collected.push((drive_letter(&instance), raw.clamp(0.0, 100.0)));
            }
            let best = busiest_from_items(&collected);
            self.last = best.clone();
            best
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for DiskBusy {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}

// DiskBusy 内含 PDH 原生句柄（裸指针）；仅在同一采集任务内访问（句柄本身是
// 进程级资源，跨线程使用安全），故标记 Send 以随 async 任务移动。
#[cfg(target_os = "windows")]
unsafe impl Send for DiskBusy {}

/// 非 Windows 平台：暂不支持磁盘忙碌时间（`new` 即失败 → 适配器不产出该 series）。
#[cfg(not(target_os = "windows"))]
pub(crate) struct DiskBusy;

#[cfg(not(target_os = "windows"))]
impl DiskBusy {
    pub(crate) fn new() -> Result<Self, String> {
        Err("非 Windows 平台暂不支持磁盘忙碌时间（M2.3.1 Windows 先行）".into())
    }

    pub(crate) fn sample(&mut self) -> Option<(String, f64)> {
        None
    }
}

/// 读取 `PCWSTR`（*const u16）指向的以 0 结尾的 UTF-16 字符串（防御性上限 256 字符）。
#[cfg(target_os = "windows")]
fn decode_wide(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while len < 256 && unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busiest_excludes_total_and_picks_max() {
        let items = vec![
            ("0 C:".to_string(), 12.0),
            ("_Total".to_string(), 95.0), // 聚合项必须排除
            ("1 D:".to_string(), 87.0),
            ("2 E:".to_string(), 3.0),
        ];
        let best = busiest_from_items(&items).unwrap();
        assert_eq!(best.0, "1 D:");
        assert_eq!(best.1, 87.0);
    }

    #[test]
    fn busiest_empty_returns_none() {
        assert!(busiest_from_items(&[]).is_none());
        assert!(busiest_from_items(&[("_Total".to_string(), 90.0)]).is_none());
    }

    #[test]
    fn busiest_picks_max_on_ties() {
        let items = vec![("0 C:".to_string(), 50.0), ("1 D:".to_string(), 50.0)];
        // 平局时任一最忙盘皆合法（max_by 语义返回最后一个相等最大值）
        let best = busiest_from_items(&items).unwrap();
        assert_eq!(best.1, 50.0);
        assert!(best.0 == "0 C:" || best.0 == "1 D:");
    }

    #[test]
    fn drive_letter_extraction() {
        assert_eq!(drive_letter("0 C:"), "C:");
        assert_eq!(drive_letter("1 D:"), "D:");
        assert_eq!(drive_letter("C:"), "C:");
        // 无盘符的实例名（如 NVMe 无卷标 / 网络盘）回退原名
        assert_eq!(drive_letter("nvme0n1"), "nvme0n1");
        assert_eq!(drive_letter("_Total"), "_Total");
        // 空格分隔的盘符不在首 token
        assert_eq!(drive_letter("3 C: D:"), "C:");
    }

    /// 实机探针（`--ignored --nocapture` 手动运行）：验证 Windows PDH 忙碌时间
    /// 空闲时不为恒 100%。非 Windows 平台无 DiskBusy 实现，此测试不编译。
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "需要真实 Windows 环境与磁盘活动，手动运行"]
    fn probe_windows_disk_busy() {
        let mut busy = DiskBusy::new().expect("PDH 初始化失败");
        std::thread::sleep(Duration::from_millis(1200));
        for tick in 0..12 {
            std::thread::sleep(Duration::from_millis(500));
            match busy.sample() {
                Some((name, pct)) => {
                    println!("tick {tick:2}: 最忙盘 = {name:<6} 忙碌 = {pct:6.2}%")
                }
                None => println!("tick {tick:2}: 无有效样本"),
            }
        }
    }
}
