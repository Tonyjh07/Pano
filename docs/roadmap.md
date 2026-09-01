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

## M1.1 —— UI 多窗口 + 托盘常驻（用户新需求）✅（交付 `a312b04`；实测暴露性能问题 → M1.2 迁移 Tauri）

在 M1 基础上重构 UI 窗口模型（设计见 `docs/ui.md` §2 / §9）：

- **多窗口**：监控组件窗口（每个信息 / 曲线 UI 组件一个独立窗口）+ 管理窗口（适配器 + 设置合并，页签切换）；
- **托盘常驻**：应用驻留系统托盘（`tray-icon`），关闭窗口 = 隐藏不退出；托盘菜单打开 / 聚焦窗口、退出应用；
- **组件级窗口自由**：UI 组件声明窗口需求（标题 / 尺寸 / 置顶 / 全屏），pano-ui 窗口管理器统一调度 viewport；
- **置顶 / 全屏**：窗口内切换按钮，运行期 `ViewportCommand::WindowLevel` / `Fullscreen` 动态切换；
- 架构影响为零（core / adapters 不变）；`pano-ui` 内部重构（单窗口 → 窗口管理器 + 托盘）。

**实现说明 / 遗留**：

- 应用图标与托盘图标：`pano-ui/assets/pano_icon.png`（512×512，include_bytes! 嵌入）；托盘图标缩放到 32×32；
- 组件窗口位置记忆（重启后恢复窗口布局）→ 由 M1.2 窗口服务的**布局持久化**承接；
- 适配器 `stop` 的 `block_on` 等待（修复一致性测试竞态）会让 UI 线程在有长清理逻辑的适配器上短暂阻塞——M1.2 迁 Tauri 后启停走异步命令，天然规避；
- `pano-core` 测试 `FakeAdapter::stop` 仍是 abort-only，存在同款偶发竞态（未触发过），后续修复。

## M1.2 —— 架构重构：Tauri 迁移 + 窗口服务 + 远程数据源预留（用户新需求）✅

**背景 / 动机**：M1.1 用户实测发现 egui/eframe 架构性限制——单事件循环 + 即时模式全量重绘 + Windows 同步渲染 hack（[egui PR #2280](https://github.com/emilk/egui/pull/2280)）：管理窗口鼠标高频交互持续 ~100fps 渲染，挤占组件窗口重绘请求（实测空隙最长 16s；渲染逻辑本身 0-2ms，瓶颈在调度）。逐项配置（Fifo / 帧延迟 / 唤醒节流）无法根治 → 用户决策迁移 **Tauri v2**（每窗口独立 WebView 渲染进程，架构根治）。

**用户新需求（决策已逐项确认）**：

- **R1 组件与窗口分离**：组件只描述内容 + `WindowSpec` 声明；窗口由主程序管理的窗口服务（新增 `pano-window`）统一创建 / 控制；保持 1 组件 = 1 窗口；
- **R2 窗口控制 API**：经 `WindowHandle` 调用全屏 / 置顶 / 绑定显示器 / 位置大小 / 可见性；「绑定显示器」= 记住上次显示器 / 位置（布局持久化）+ 可指定显示器打开；仅 Rust 内部 API（不预留远程控制协议）；
- **R3 远程数据源预留**：HTTP 轮询 + WebSocket 推送都预留；core 定义轻量 `HttpClient` 抽象（不依赖具体库）+ pano-adapters 提供共享基座（reqwest / tokio-tungstenite）；新增能力标记 `RemoteSource`；不实现具体远程适配器。

**范围**：

- 新增 `pano-window` crate：`WindowService` / `WindowHandle` / `WindowSpec` / `MonitorId` / 布局持久化（Tauri v2 实现）；
- `pano-ui` 重写：Rust 命令层（tauri commands + 事件桥接 core→前端）+ 前端（推荐 Svelte 5 + Vite + TS + uPlot，可替换，见 ui.md §10）；
- `pano-core` 扩展：`UISpec` 增加 `components` 声明；`AdapterContext` 增加 `http` 注入点；`Capability::RemoteSource`；
- `pano-adapters`：新增 `remote/` 共享基座（`http_poll` / `ws_push` 模板）；
- `pano-app`：装配 core + 窗口服务 + ui；托盘常驻、`--log-file` 延续；
- 测试与文档：pano-window 单测（spec / 持久化）、命令层单测、前端 Vitest；`docs/` 四份设计文档与 `AGENTS.md` 已更新为 M1.2 设计基线（设计先行）。

**设计文档（本里程碑基线）**：`docs/architecture.md`（§1/§5/§7/§13/§14）、`docs/ui.md`（§2/§6/§9/§10）、`docs/spec.md`（§2/§3/§8/§9/§10）、本文档。

**实现说明 / 遗留**：

- egui 版本（`a312b04` 及其后未提交的修复批次）按用户决策**直接丢弃**，不保留回退分支；已验证经验已吸收进本里程碑设计：修改即时生效、启停 / 配置修改走后台异步（不卡 UI）、关闭窗口 = 隐藏、数据驱动刷新用事件推送（非轮询 / 非帧内检测）、`--log-file`；
- **前端栈确认落地**：Svelte 5 + Vite + TS + uPlot（编码前决策确认）；Tauri 壳并入 `pano-app`（决策确认，见 architecture §11 注）；
- **tauri CLI 注意**：before 命令以 `crates/` 为工作目录执行（frontend 目录深度 3 查找回退），`tauri.conf.json` 的 beforeDevCommand / beforeBuildCommand 使用相对 `crates/` 的路径（`pnpm --dir pano-ui/web …`），`cargo tauri dev` 从仓库根运行；
- **远程数据源仅预留基座与文档**：`remote/` 模板按 feature 编译（`remote-http` / `remote-ws`），模板内置简易退避（core 级自动退避归 M2）；具体远程适配器归 M2+ 按需实现，接入路径见 architecture §14；
- **布局持久化**：`[window.<id>]` 段由 pano-app 协调串行写回（窗口移动 / 缩放实时记内存，退出 / 配置保存时段级合并写文件）；记忆位置优先于显示器偏好，失效显示器回退默认位置不硬失败；
- **打包**：`bundle.active = false`（M3 打包时再开）；`SeriesId::parse` 依赖调用方保证格式；同名显示器 MonitorId 碰撞为已知边界；
- 一个窗口多组件布局明确不在本次范围。

## M2 —— 监控适配器与仪表盘

- 系统监控适配器：`sys.cpu`、`sys.mem`、`sys.disk`、`sys.net`（按平台条件编译，共用 `sysinfo`，Windows / Linux / macOS）；✅
- 默认特性切换为系统监控适配器（spec §5：M2 起默认只含系统监控适配器）；✅
- 一致性测试基座适配多 series 适配器（`run_all` 增加 series 参数，轮询等待首样本）；✅
- 远程数据源示例适配器（复用 M1.2 基座，如 HTTP 轮询型 / WebSocket 推送型各一个示例）；
- 仪表盘正式化：多面板布局、速率图 / 曲线；
- CI 门禁（fmt / clippy / test / audit）；前端 Vitest + 命令层单测；
- 主题切换落地。

**实现说明 / 遗留（sys 适配器采样节奏，诊断记录）**：

- **sysinfo 首次刷新慢**：Windows 实测 CPU `refresh_cpu_usage()` 首次约 1s（冷启动）。适配器在 `start` 中**预热**刷新一次（丢弃结果），避免冷启动延迟污染采样节奏；
- **interval 补爆**：`tokio::time::interval` 默认 `MissedTickBehavior::Burst` 会一次性**补发**错过的 tick（一次慢阻塞后连续补发）→ 采样间隔失真（实测 5.8ms vs 期望 200ms）；sys 适配器统一 `Skip`；
- **不用 `block_in_place`**：sys 适配器任务内不调用 `tokio::task::block_in_place`（短阻塞直接同步调用）——block_in_place 与 time driver 在运行时关闭时存在竞态 panic（「A Tokio 1.x context was found, but it is being shutdown」），workspace 并行测试时触发；
- **一致性测试基座**：`run_all` 从「固定查 `<adapter>.value`」改为接收 series 参数（主指标），并**轮询等待**主 series 达 3 样本（超时 = `max(sampling*8, 3s)`），吸收首样本延迟；
- 磁盘 / 网络设备集合依赖平台差异，`device` / `interface` 过滤子串可能命中多个设备；跨平台行为待实机验证；
- 仪表盘正式化（多面板布局、速率图）、CI `cargo audit` 归后续。

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
| 前端曲线大量数据掉帧 | uPlot + 样本抽稀（只取最近 N 点）+ 事件节流 |
| tokio 与 Tauri 生命周期错位 | M1 定义清楚关闭顺序：先停适配器任务，再退窗口 / 结束进程 |
| 环形缓冲锁竞争 | M1 简单锁起步，预留 arc-swap / 分片锁替换点 |
| 平台差异导致适配器行为不一 | 一致性测试 + Unsupported 状态机制 |
| Windows 缺 WebView2 运行时 | 安装包引导安装 / 检测提示（M3 打包时落地） |
| 前端构建链引入复杂度 | 前端栈收敛在 `pano-ui/web/` 内部；命令层与前端经类型化接口对接 |
