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

## M2.1 —— 修复与窗口管理（用户新需求）

- 适配器 `stop()` 在 tokio 运行时内不再 panic（启用 / 停用热生效路径）✅
- 管理窗口窗口管理功能：新建 / 分配适配器 / 切换适配器 / 隐藏 / 销毁窗口；
- 托盘窗口列表动态化（取代静态「监控组件窗口」项）。

**实现说明 / 遗留（stop 运行时上下文修复，诊断记录）**：

- **根因**：`set_adapter_enabled` / `set_adapter_sampling` / `restart_adapter`（pano-ui `commands.rs`，均为 async Tauri 命令）运行于 tokio 运行时 worker 线程内，命令链 `apply_config → Lifecycle::apply_adapter_config → adapter.stop()` 中，各适配器 `stop()` 原实现调用 `Handle::block_on(task)`——在运行时内调用 `Handle::block_on` 必 panic（「Cannot start a runtime from within a runtime」），用户实测启用 / 禁用适配器即触发；
- **修复**：新增 `pano-adapters::util::shutdown_task`，`abort` 后用 `Handle::try_current()` 区分上下文——非运行时线程（一致性测试 / 应用退出主线程）走 `block_on` 阻塞等待（保持「stop 后不再产出样本」语义）；运行时内（async 命令路径）仅 `abort`（任务在下一 await 点被取消，至多 1 个在途样本，环形缓冲丢弃，UI 侧无害）；
- **回归测试**：`shutdown_task` 单测覆盖「运行时内调用 stop 不 panic 且任务被取消」（一致性测试走非运行时线程路径，无法覆盖运行时内分支，故补专门测试）；
- 全局 `block_in_place` 仍未在任务内使用（遵守 §M2 教训）；本次修复在 `stop()` 内、用 `try_current` 区分上下文，不涉 time driver 关闭竞态。

**实现说明 / 遗留（窗口管理「新建窗口」卡死，诊断记录）**：

- **根因**：`create_window` 最初是**同步命令**，在 WebView2 `ipc://` 自定义协议回调（`WebResourceRequested` 事件，主线程事件循环内）里直接调用 `WebviewWindowBuilder::build()`。Windows 上创建 WebView2 控制器用 `wait_with_pump` 在当前线程跑**嵌套消息循环**等待完成回调，而当前线程正被同步命令占用（WebView2 单线程模型禁止在自身回调内同步创建新 WebView）→ 嵌套泵永远等不到回调 → 整个后端卡死。Tauri 文档明确警告 [wry#583](https://github.com/tauri-apps/wry/issues/583)：「On Windows, this function deadlocks when used in a synchronous command or event handlers; You should use `async` commands and separate threads when creating webviews」；
- **修复**：`create_window` 改为 **async 命令**。async 命令经 `respond_async_serialized` 在 tokio 线程执行；`build()` 在非主线程调用时 `send_user_message` 走 `proxy.send_event`（非阻塞排入主线程事件队列）并立即返回 `DetachedWindow`，真正建窗在主线程事件循环里执行（脱离 WebView2 IPC 回调上下文）——与 Tauri 官方 `create_webview_window` async 命令行为一致；
- **托盘重建线程**：`refresh_tray_now()` 在 async 命令（tokio 线程）里调用 → `TrayIcon::set_menu` 的 `run_item_main_thread` 阻塞等待主线程执行菜单任务；主线程空闲时会处理，不构成死锁（仅短暂占用 tokio worker）。`destroy_window`（同步命令，主线程内）调 `refresh_tray_now` 时 `send_user_message` 检测到主线程直接同步执行，同样安全；
- 启动建窗（pano-app setup）在主线程 setup 回调里，不在 WebView2 IPC 回调上下文中，安全，无需改动。

## M2.2 —— UI 组件（用户新需求）

**设计**：`docs/ui.md` §2/§3.2/§4、`docs/architecture.md` §7（窗口 = 组件类型实例）、本文档。方案 A（用户确认）——**UI 组件 = 自带固定 series 的模板**，取代「窗口直接指定 series」。

- `UISpec.components` 即**组件目录**：每个 `ComponentSpec` 含 `id` / `name` / 固定 `series` / 默认 `window`；
- 监控窗口 = 组件类型实例：`WindowEntry` 改存 `{ title, component }`（**不再直接存 series**），series 由目录解析；多窗口可绑定同一组件；切换组件 = 仅换内容（`pano://window-component` 事件重载），标题 / 几何不动；
- 命令层：`list_components`（含 `available`：全部 series 所属适配器已注册；未注册组件置灰）、`window_content`、`create_window(id, component, title?)`、`set_window_component(id, component)`（替代 `set_window_series`）；
- **窗口集合持久化**（用户确认）：`[ui].windows` 段 = 监控窗口 id 列表（**段缺失** = 首次运行按目录播种并写回；**空列表** = 用户删光窗口保持为空）；`[window.<id>]` 段持久化 `component` + `title` + 布局；运行时新建 / 切换 / 销毁写回配置，重启恢复；
- 默认组件目录基于 sys 适配器（cpu / mem / disk / net 各一组件），示例组件保留（默认 feature 下置灰不可选）；
- 前端：`renderers.ts` 按组件 id 分派渲染器；M2.2 通用渲染 = 组件 series 逐个「数值卡 + 曲线」（`TimeSeriesPanel.svelte`）。

**实现说明 / 遗留**：

- **前端单测缺口延续**：命令层校验逻辑抽纯函数并补 Rust 单测；前端 `renderers` 渲染器分派暂以 Vitest 覆盖纯函数，组件渲染级单测（Svelte 组件测试方案）此前已弃用（见 M2.1 记录）；
- **M2.3 汽车仪表盘组件**：见下文「M2.3 —— 汽车仪表盘式资源监控」；
- **配置升级**：M2.1 既有 `[window.<id>]` 布局段在 M2.2 首次运行时因缺 `[ui].windows` 段会触发按目录播种并写回（旧布局段被覆盖为组件 + 布局）；已知边界，文档注明；
- `component_series` 命令更名为 `window_content`（返回 component + series）；`pano://window-series` 事件更名为 `pano://window-component`；
- 动态窗口「仅布局记忆、集合不持久化」的 M2.1 未定项已在本里程碑按用户选择定稿为「集合持久化」。

## M2.3 —— 汽车仪表盘式资源监控（用户新需求）

**设计**：专用 UI 组件 `sys-dashboard`（`renderers.ts` 注册专用 Svelte 渲染器），验证「换组件 = 换渲染」的可替换性。用户确认三项数据源决策。

**数据源增强（sys 适配器）**：

- `sys.disk.active_percent`（Number，%）：**磁盘活动率**（近似「磁盘忙碌时间」）。以 sysinfo `DiskUsage.read_bytes + written_bytes`（自上次刷新的增量）判定该采样点是否有读写 IO；滑动窗口取最近 10 个采样点中活跃比例 × 100（活动率纯函数可单测）；
- `sys.net.utilization`（Number，%）：**链路利用率** =（recv_bps + sent_bps）/（参考带宽）× 100，钳制 0..=100；参考带宽 `link_mbps` 可配（Number，默认 1000 = 1 Gbps；`1 Mbps = 125_000 B/s`）；
- 四个 sys 适配器各增配置字段 `high_threshold`（Number，默认 80，合法域 0..=100）：**高占用阈值**（仪表盘指示灯判定用；适配器自身不使用该值，仅作为数据源配置暴露）。前端经 `list_adapters` 返回的 `config`（当前自定义配置值）读取，未配置回落默认 80。

**组件目录**：`sys-dashboard`（name「资源仪表盘」），固定 series = `sys.cpu.usage` + `sys.mem.used_percent` + `sys.disk.active_percent` + `sys.net.utilization`；默认窗口标题「资源仪表盘」、尺寸 860×540。

**专用渲染器 `SysDashboard.svelte`**（`renderers.ts` 注册 `sys-dashboard`）：

- 汽车仪表盘式布局：左右两个**大仪表**（CPU、内存占用率），下方两个**小仪表**（磁盘活动率、网络利用率）；SVG 半圆弧刻度 0-100 + 指针 + 红区（阈值→100）；
- 每个仪表下方一个**指示灯**：该指标占用率超其适配器 `high_threshold`（缺省 80）→ 亮红，否则绿色；
- 数据取自通用窗口管道（ComponentWindow 传 seriesList / samples / latest），仪表显示 `latest` 瞬时值；`Gauge.svelte` 为可复用 SVG 半圆仪表（参数化大小 / 阈值 / 红区）。

**验收**：`cargo tauri dev` 后在「窗口管理」新建窗口绑定「资源仪表盘」（或删除 pano.toml 中的 `[ui]` / `[window.<id>]` 段——段缺失 = 首次运行，触发按目录播种并写回）；四个仪表实时反映占用，任一指标超阈值其指示灯变红。

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
