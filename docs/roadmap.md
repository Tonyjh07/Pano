# Pano 里程碑与实施计划

## M0 —— 设计（本次交付 ✅）

- 三份设计文档：规范设计、架构设计、图形界面设计；
- 关键决策：trait 注册表 + 特性开关 / 推送 + 环形缓冲 / tokio / 初期范围 = 管理界面 + 示例适配器。

## M1 —— 架构骨架（初期范围：管理界面与示例）✅

目标：跑通「适配器 → core → UI」全链路，**可替换性得到验证**。

- workspace 与四个 crate 骨架；✅
- pano-core：Adapter trait、注册表、生命周期、环形缓冲 SampleStore、配置加载；✅
- pano-adapters：示例适配器 `example.counter`、`example.sine`（feature 开关）；✅
- pano-ui：侧边栏框架 + 三个页面骨架 + 适配器管理页（启停、采样间隔、状态徽标、schema 表单）+ 示例仪表盘（数值卡、实时曲线）；✅
- pano-app：启动流程（配置 → 能力校验 → 挂载 UI）；✅
- 适配器一致性测试基座，两个示例适配器全部通过；✅
- **验收标准**：换 feature 能替换适配器；管理页可启停并热生效。✅

**实际落地差异 / 遗留**：

- egui 采用 0.36（2026 新 API：`App::ui`、统一 `Panel`、无 `Switch` 用 Checkbox 替代）；egui_plot 0.37 对应 egui 0.36；
- 适配器运行期故障的**自动退避重试**（架构 §9）未实现——M1 提供 Error 状态 + 手动重启；自动重试归 M2；
- 管理页 schema 表单仅渲染 + `enabled`/`sampling` 可编辑热生效；自定义参数编辑生效与失败回滚归 M3（架构 §8）；
- 设置页为骨架（主题切换 / 默认采样 / 缓冲容量归 M3）。

**M1 审查建议待办（已 APPROVE，非阻断）**：

- `CoreError` 增加 `Adapter(#[from] AdapterError)` 变体，避免 start/stop 失败一律报「配置错误」（错误分类失真）；
- 热生效失败回滚：`apply_adapter_config` 应先校验启动成功再提交内存（或失败恢复旧配置）；UI 写回顺序改为先写文件再应用；
- adapter id 格式校验抽共享函数（注册 + 配置解析两处复用，含全小写 ASCII 校验）；
- 统一时钟源：core 暴露 `now()` 助手供适配器取时间戳（架构 §4「时间戳由 core 提供」）。

## M2 —— 监控适配器与仪表盘

- 系统监控适配器：`sys.cpu`、`sys.mem`、`sys.disk`、`sys.net`（按平台条件编译）；
- 仪表盘正式化：多面板布局、速率图 / 曲线；
- CI 门禁（fmt / clippy / test / audit）；`egui_kittest` 无头 UI 测试；
- 主题切换落地。

## M3 —— 增强与打磨

- 配置热重载完善（自定义参数变更 + 回滚机制）；
- 日志查看页（tracing 面板内展示）；
- i18n 文案 key 抽取；
- 打包分发（Windows / macOS / Linux）。

## M4 —— 插件化（预留）

- 依据 M1 的边界设计评估拆 cdylib 动态加载；不承诺时间点。

## 风险与对策

| 风险 | 对策 |
| --- | --- |
| egui 大量曲线时掉帧 | 样本抽稀、帧率上限、仅数据变化时重绘 |
| tokio 与 egui 生命周期错位 | M1 定义清楚关闭顺序：先停适配器任务，再退 UI |
| 环形缓冲锁竞争 | M1 简单锁起步，预留 arc-swap / 分片锁替换点 |
| 平台差异导致适配器行为不一 | 一致性测试 + Unsupported 状态机制 |
