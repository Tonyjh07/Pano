//! 适配器共享工具（M2.1 修复：`stop` 在 async 命令路径不再 panic）。

use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// 停止采集任务：`abort` 后视当前线程是否在 tokio 运行时内决定是否阻塞等待。
///
/// - 当前线程**不在** tokio 运行时内（一致性测试线程 / 应用退出主线程）：
///   阻塞等待任务真正结束，保证 `stop` 返回后不再产出样本（一致性测试
///   「生命周期检查」依赖此语义）；
/// - 当前线程**在** tokio 运行时内（async 命令路径，如启用 / 停用热生效）：
///   `Handle::block_on` 会 panic（`Cannot start a runtime from within a
///   runtime`），故仅 `abort` —— 任务在下一 await 点被取消，返回后至多
///   1 个在途样本（UI 侧无害，环形缓冲自动丢弃旧样本）。
///
/// 根因与修复见 roadmap §M2.1。
pub fn shutdown_task(task: Option<JoinHandle<()>>, runtime: Option<&Handle>) {
    let Some(task) = task else {
        return;
    };
    task.abort();
    if Handle::try_current().is_err()
        && let Some(rt) = runtime
    {
        let _ = rt.block_on(task);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// 构造一个「持续递增计数 + 长 sleep」的采集任务（模拟适配器采集循环）。
    fn spawn_loop(rt: &tokio::runtime::Runtime, counter: Arc<AtomicUsize>) -> JoinHandle<()> {
        rt.spawn(async move {
            loop {
                counter.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        })
    }

    /// 回归：**在 tokio 运行时内**调用 `shutdown_task`（async 命令路径，
    /// 如启用 / 停用热生效 → `adapter.stop()`）不得 panic，且任务被取消
    /// （此前 `Handle::block_on` 在此上下文必 panic「Cannot start a runtime
    /// from within a runtime」，见 roadmap §M2.1）。
    #[test]
    fn shutdown_task_within_runtime_does_not_panic_and_cancels() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = rt.handle().clone();
        let counter = Arc::new(AtomicUsize::new(0));
        let task = spawn_loop(&rt, Arc::clone(&counter));

        // 任务先跑一会儿，确认计数在增长
        std::thread::sleep(Duration::from_millis(20));
        assert!(counter.load(Ordering::Relaxed) > 0, "任务应已开始采集");

        // 关键：在运行时内调用（与 async 命令一致），不应 panic
        rt.block_on(async {
            shutdown_task(Some(task), Some(&handle));
        });

        // 任务应被取消：计数不再增长（abort 在下一 await 点生效）
        std::thread::sleep(Duration::from_millis(20));
        let after = counter.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            counter.load(Ordering::Relaxed),
            after,
            "abort 后任务应停止推进（已被取消）"
        );
    }

    /// 非运行时线程（测试线程 / 应用退出主线程）调用：阻塞等待任务真正结束。
    #[test]
    fn shutdown_task_outside_runtime_blocks_until_task_ends() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let task = spawn_loop(&rt, Arc::clone(&counter));

        std::thread::sleep(Duration::from_millis(20));
        assert!(counter.load(Ordering::Relaxed) > 0);

        // 测试线程（非运行时上下文）调用：block_on 分支，阻塞至任务结束
        shutdown_task(Some(task), Some(rt.handle()));

        // block_on 已等任务真正结束，计数同样不再增长
        std::thread::sleep(Duration::from_millis(20));
        let after = counter.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(counter.load(Ordering::Relaxed), after);
    }
}
