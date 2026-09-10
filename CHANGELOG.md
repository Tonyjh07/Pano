# Changelog

## [Unreleased] — M2.5

### M2.5 DeepSeek API 额度监控（分支 dev）

- **远程适配器 `deepseek.balance`**（feature `adapter-deepseek-balance`，**R3 注入首次落地**）：按采样周期轮询 DeepSeek 官方 `GET {base_url}/user/balance`（`Authorization: Bearer <api_key>`）；series = `total` / `granted` / `topped_up` × `cny` / `usd`（Number，按响应实际币种产出）+ `is_available`（Bool）；自定义配置 `api_key`（env `PANO_DEEPSEEK_API_KEY` 优先，回落配置段）/ `base_url` / `low_threshold`（低余额阈值）；复用 `remote/http_poll::poll_loop`（退避重试 + Error 阈值）；pano-app 按 feature 构造 `ReqwestHttpClient` 经 `Lifecycle::with_http` 注入 core。
- **安全**：`list_adapters` 与 `config_preview` 对密钥类键 `api_key` 读回掩码（`********`），密钥不进 WebView。
- **UI 组件 `deepseek-balance`（「DeepSeek 额度」）**：专用渲染器 `DeepSeekBalance.svelte`——状态条（账户可用 / **高峰时段 · 价格翻倍**（周一~周五 9:00~12:00、14:00~18:00，本地时间判定）/ 低余额指示灯 / 适配器 Error 提示）+ CNY 主卡（大数字 + 充值/赠送细分 + **近 5 分钟 / 近 30 分钟消耗**）+ USD 副卡 + uPlot 趋势曲线；前端纯函数 `isPeakHour` / `consumptionInWindow` + Vitest。
- **测试**：loopback 假 DeepSeek 服务（`std::net::TcpListener`，零新增依赖）一致性测试 + 单元测试（JSON 解析 / 配置校验含 env 回落）；前端 Vitest 62 例全绿；`cargo fmt` / `clippy -D warnings` / `cargo test` / `svelte-check` / `vite build` 门禁通过。

## [0.2.0] — M2.1 ~ M2.4 窗口管理与汽车仪表盘（`tag v0.2.0`）

### M2.4 UI 升级：汽车仪表盘 + 小屏自适应 + 无边框拖拽（分支 m2.2）

- **专用渲染器 `sys-dashboard`**：`SysDashboard.svelte` 汽车仪表盘式布局——上方两个大仪表（CPU / 内存占用率）、下方两个小仪表（磁盘活动率 / 网络利用率）；SVG **240° 弧形表盘**（12 点位为 0°、顺时针为正、行程 -120° → +120°，0 在 8 点 / 50 在 12 点顶部 / 100 在 4 点），含外圈金属边框、长短刻度、0/25/50/75/100 标签、红区（阈值→100）、指针 + 尾翼、中心 hub、霓虹辉光、指示灯（超 `high_threshold` 亮红）；组件名印于表盘内。
- **小分辨率自适应（动态适配）**：`ResizeObserver` 实时测量容器，**布局随可用空间推导、无硬编码尺寸**——高度 ≥120px 常规 2×2（大/小表盘比例 1:0.73 按宽高缩放，上限 300）；<120px 紧凑（按宽高比定列数：宽横屏 4 列一排 / 近方 2×2，表盘 = `min(列宽, 行高预算)`）；极端矮（<50px）隐藏读数行只留表盘。支持 **400×100 小屏（含 Windows 125% 缩放 → 逻辑 320×80）**。
- **修复（用户实测三轮回归）**：① 无边框窗口无法拖动——capabilities 增 `core:window:allow-start-dragging`（`core:window:default` 默认不含）；② 小屏仍不适配——建窗按目标显示器工作区收敛尺寸（`fit_size_to_work_area`）+ 窗口尺寸物理/逻辑像素按 scaleFactor 换算；③ 125% 缩放下紧凑表盘垂直裁切——`Gauge` 紧凑样式缩减垂直开销；④ 紧凑布局不触发的根因——`ComponentWindow` 的 `main` 设 `min-height:0`（flex 子项默认 `min-height:auto` 会把容器撑开，RO 测到的是内容高度而非可视高度）。
- **无边框窗口 + 内容拖拽**：`WindowSpec.decorations`（组件声明，`sys-dashboard` 默认无边框）+ `[window.<id>].decorations` 窗口级覆盖；「窗口管理」页无边框开关（`window_set_decorations`：即时切换 + 写回 + 定向 emit）；无边框窗口内容区（`data-pano-drag` + 壳 header）mousedown 委托 `startDragging` 移动窗口，组件只声明标记、平台调用收敛在窗口壳。
- **`Gauge` 可复用组件**：`compact` prop（紧凑样式缩减垂直开销）+ `readout` 开关（极端矮隐藏读数行）。

### M2.3.1 磁盘仪表显示最忙盘符（分支 m2.2）

- **`sys.disk` 活动率改为最忙盘真实忙碌时间**：Windows 用 PDH `%DiskTime` 取最忙盘，产出 `busiest_disk`（盘符）与 `active_percent`（活动率）。
- **`sys-dashboard` 订阅 `sys.disk.busiest_disk`**：磁盘仪表显示当前最忙盘符与活动率（`Gauge` 副读数 `detail` 行）；非文本 / 缺失回落。

### M2.3 汽车仪表盘式资源监控（分支 m2.2）

- **数据源扩展**：`sys.disk` 新增磁盘活动率、`sys.net` 新增链路利用率 series；适配器自定义配置 `high_threshold`（高占用阈值，默认 80，`sys.net` 另加 `link_mbps` 参考带宽）。
- **UI 命令层下发适配器自定义配置**：`list_adapters` 返回 `config` 字段，前端 `configNumber` 读取阈值。
- **`sys-dashboard` 目录项 + 专用渲染器**：注册 `sys-dashboard` 组件，`renderers.ts` 分派 `SysDashboard.svelte`；前端测试与 vitest Svelte 5 挂载修复（`test/setup.ts` matchMedia stub）。

### M2.2 UI 组件化（分支 m2.2）

- **组件数据模型**：`ComponentSpec.name` 命名组件；`[ui].windows` 持久化监控窗口 id 列表；`[window.<id>].component` / `title` 绑定组件与标题覆盖；首次运行按目录播种默认监控窗口。
- **组件命令层与前端**：组件目录 / 窗口绑定与切换 / 渲染分派（`renderers.ts` 按组件 id 选渲染器，通用渲染 = 数值卡 + uPlot 曲线 `TimeSeriesPanel.svelte`）+ 窗口集合持久化装配。

### M2.1 窗口管理（分支 dev，M2 后追加）

- **窗口管理核心 API**：`Adapter::series` 扩展、`WindowService::destroy` / `is_visible`。
- **运行时注册表 + 命令层 + 托盘动态窗口列表 + 前端窗口页签**：窗口管理页可新建 / 切换 / 关闭组件窗口，托盘同步动态列表。

### M2 修复与流程

- 适配器 `stop` 在 tokio 运行时内不再 panic（启用 / 停用热生效路径）；配置文件路径从 cwd 逐级向上解析，修复 `cargo tauri dev` 启动失败；`sys_dashboard` 目录测试编译修复。

## [0.1.0] — M1.x / M2 开发期（未发布）

### M2 系统监控适配器（分支 m1.2，开发中）

- **新增 `sys.cpu` / `sys.mem` / `sys.disk` / `sys.net` 系统监控适配器**：共用
  `sysinfo`，按平台条件编译（Windows / Linux / macOS）；自定义字段
  （`per_core` / `swap` / `device` / `interface`）经 `config_schema` 渲染。
- **默认特性切换**：`pano-app` / `pano-adapters` 默认只含系统监控适配器（spec §5）；
  示例 / 远程适配器需显式 feature 启用。
- **一致性测试基座适配多 series**：`run_all` 增加 series 参数（主指标），轮询
  等待首样本（超时 = `max(sampling*8, 3s)`），适配 sysinfo 首次刷新冷启动延迟。
- **sys 适配器采样节奏修复**（诊断记录见 roadmap §M2）：`start` 预热刷新；
  interval 用 `MissedTickBehavior::Skip`（避免补爆）；任务内不用 `block_in_place`
  （避免与 time driver 的运行时关闭竞态 panic）。

### M1.2 架构重构：Tauri 迁移 + 窗口服务 + 远程数据源预留（分支 m1.2）

- **新增 `pano-window` crate**：`WindowService` / `WindowHandle` / `MonitorId` /
  布局持久化（`[window.<id>]` 段），Tauri v2 实现（`WebviewWindow` 封装）；
  窗口控制一律经 `WindowHandle`，命令层与组件不直接操作 Tauri 类型；
  建窗定位决策（`persist::placement_of`）：记忆位置优先，失效显示器回退默认不硬失败。
- **pano-ui 重写为 Tauri 命令层 + Svelte 5 前端**：命令层（适配器启停 / 采样 /
  窗口控制 / 快照查询）、事件桥接（`SampleStore` 订阅 → 前端 `listen`，无轮询）、
  前端（多窗口 SPA：管理窗口页签 + 组件窗口 uPlot 实时曲线，Vitest 10 例）；
  egui 界面整版移除（M1.1 实测性能问题根治，见 roadmap §M1.1）。
- **pano-core 扩展**：`UISpec.components`（`ComponentSpec` / `WindowSpec` 纯数据声明）、
  `Capability::RemoteSource`、`HttpClient` 抽象（`HttpResponse` / `HttpError`，dyn-compatible）、
  `AdapterContext.http` 注入点、`SampleStore` 事件订阅（broadcast）、
  `[window.<id>]` 布局持久化配置段、`CoreError::Adapter` / `InvalidAdapterId`、
  `apply_adapter_config` 失败回滚、统一时钟源 `now()`、adapter id 共享校验。
- **pano-adapters 远程数据源基座（R3 预留）**：`remote/http_poll`（reqwest 实现
  `HttpClient` + 轮询骨架）、`remote/ws_push`（断线重连骨架 + 连接状态映射），
  简易指数退避（1s/2s/4s…封顶 60s）；具体远程适配器归 M2。
- **pano-app 装配**：Tauri Builder（命令注册 / 托盘 / 关闭=隐藏 / 布局持久化
  串行写回 / 事件桥接 / headless 模式保留 / `--log-file`）。
- **修复（dev 流程）**：`cargo tauri dev` 的 cwd 与仓库根不一致导致默认配置
  `pano.toml` 找不到——配置文件路径改为从 cwd 逐级向上解析；README「快速开始」
  区分开发模式（`cargo tauri dev` 一条命令）与独立运行（release 构建嵌入前端
  资源，不依赖 dev server）。

### M1.1 UI 多窗口 + 托盘常驻（交付 `a312b04`，已被 M1.2 Tauri 迁移替代）

- egui 多窗口（管理窗口 + 组件窗口）、托盘常驻、组件级窗口置顶 / 全屏。
- 实测暴露 egui 单事件循环性能问题（详见 roadmap §M1.1），M1.2 迁移 Tauri 根治。

### M1 架构骨架

- workspace 四 crate（core / adapters / ui / app）、适配器注册表与生命周期、
  环形缓冲 SampleStore、配置加载与热生效、示例适配器 `example.counter` /
  `example.sine`、一致性测试基座、egui 管理界面。
