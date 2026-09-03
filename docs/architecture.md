# Pano 架构设计

> 本文档随 M1.2 架构重构更新：UI 迁移 Tauri v2，引入窗口服务（pano-window），预留远程数据源能力。
> 变更背景与决策记录见 `roadmap.md` §M1.2。

## 1. 总体分层

```
┌─ UI（pano-ui：Tauri 前端 + Rust 命令层）—— 可替换 ───────┐
│  · 组件 = 纯内容（渲染 + WindowSpec 声明）               │
│  · 只读访问 core 快照（事件订阅）                        │
│  · 经 WindowHandle 调用窗口 API（不持有窗口实现）         │
└──────┬─────────────────────────────┬──────────────────┘
       │ 公共只读 API（样本、状态、控制） │ 窗口 API（WindowHandle）
┌──────▼──────────────┐        ┌──────▼──────────────────┐
│ CORE（pano-core）    │        │ WINDOW（pano-window）    │
│ 不可替换的编排层      │        │ 窗口服务（pano-app 装配） │
│ · AdapterRegistry   │        │ · 窗口生命周期            │
│ · Lifecycle         │        │ · 控制 API：全屏/置顶/    │
│ · SampleStore       │        │   显示器绑定/位置/可见性   │
│ · Config/热重载      │        │ · 布局持久化（位置/显示器）│
│ · CapabilityMatcher │        └──────────────────────────┘
└──────┬──────────────┘
       │ Adapter trait（唯一契约）
┌──────▼────────────────────────────────────────────────┐
│  ADAPTERS（pano-adapters）—— 可替换，特性开关编译        │
│   example.counter / example.sine / (M2) sys.*          │
│   远程数据源基座预留（HTTP 轮询 / WebSocket 推送，§14）  │
└───────────────────────────────────────────────────────┘
```

核心不变量：

1. **适配器不知道 UI 存在**——它只向 SampleSink 推送样本；
2. **UI 不知道具体适配器**——它只按能力（Capability）和 series 取数据；
3. **UI 组件不知道窗口实现**——窗口由主程序装配的窗口服务（pano-window）管理，组件只声明 `WindowSpec`、持 `WindowHandle` 调 API（R1/R2 需求）；
4. **替换任何一侧都不改动另一侧**。

## 2. 核心概念

| 概念 | 说明 |
| --- | --- |
| `Adapter` | 一个信息源，trait 契约见 §3 |
| `AdapterId` | 全局唯一，`<域>.<名称>` |
| `Capability` | 能力标签（如 `TimeSeries`、`SystemInfo`、`RemoteSource`），UI 按能力声明需求 |
| `SeriesId` | 一条指标序列，`<adapter_id>.<metric>` |
| `Sample` | `{ timestamp, value }`；value 支持数值 / 布尔 / 文本 / JSON |
| `SampleSink` | 适配器持有的推送句柄（可 clone、多线程安全） |
| `SampleStore` | 核心持有的环形缓冲集合，UI 只读 |
| `AdapterStatus` | `Disabled / Stopped / Starting / Running / Error` |
| `UISpec` | UI 声明所需能力 + 组件清单（§7） |
| `WindowService` | 窗口管理器（pano-window），由 pano-app 装配；创建 / 控制 / 持久化窗口（§13） |
| `WindowHandle` | 组件持有的窗口控制句柄：全屏 / 置顶 / 显示器 / 位置 / 可见性（§13） |
| `WindowSpec` | 组件声明的窗口需求：标题 / 尺寸 / 置顶 / 全屏 / 显示器偏好（§13） |
| `MonitorId` | 显示器标识，用于「绑定特定显示器」（§13） |
| `ComponentSpec` | **UI 组件类型**（M2.2，`UISpec.components` 目录）：一种「窗口内容形态」，自带固定 series + 默认窗口规格（§7） |

## 3. Adapter trait（M1 定稿，M1.2 不变）

```rust
pub trait Adapter: Send + Sync {
    fn meta(&self) -> AdapterMeta;                  // 元信息（含唯一 id）
    fn capabilities(&self) -> Vec<Capability>;      // 提供的能力
    fn config_schema(&self) -> ConfigSchema;        // 自定义配置 schema（渲染表单）
    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError>;
    fn stop(&mut self) -> Result<(), AdapterError>;
    fn status(&self) -> AdapterStatus;
}
```

`AdapterContext` 提供给适配器：

- `sink: SampleSink` —— 推送样本（可 clone、多线程安全）；
- `sampling: Duration` —— 本适配器采样周期（由配置决定）；
- `runtime: tokio::runtime::Handle` —— 适配器在 `start` 内据此自建采集任务；
- `config: HashMap<String, ConfigValue>` —— 自定义配置（pano.toml 解析而来）；
- `http: Option<Arc<dyn HttpClient>>` —— **M1.2 预留**：远程数据源注入点（§14，`Option<Arc<dyn HttpClient>>` 与 §14 代码一致）。

`ConfigSchema` 只描述**自定义字段**；`enabled` / `sampling` 为 core 固定字段，不进 schema。

设计要点：

- 每个适配器**自持状态**，core 不假设其内部实现（单任务或多任务均可）；
- `start` 返回即视为已开始，适配器负责在 tokio 上自建任务循环；
- `stop` 必须干净退出（任务 join、资源释放）；stop 返回后不得再产出样本、会阻塞调用线程、不得在异步上下文调用（M1.1 修复竞态时定稿，见 roadmap M1.1 遗留）。

## 4. 数据流：推送 + 环形缓冲 + 事件订阅

```
适配器任务循环（每 sampling 一次）
  │  采集 / 计算（本地采集，或经 HttpClient 远程获取，§14）
  ▼
SampleSink.push(Sample) ──► SampleStore 对应 series 的环形缓冲
                                │（写端：适配器任务）
                                ▼
pano-ui 桥接层（Rust）订阅 SampleStore 变化
  │  任一 series 有新样本 → Tauri 事件 emit（按窗口订阅分发）
  ▼
WebView 前端组件 listen → 更新数值卡 / 实时曲线
```

- 环形缓冲默认容量按 series 可配置（默认 4096 样本），满则覆盖最旧；
- 写端设计：M1 用 `RwLock`/`Mutex` 起步，预留替换点（`arc-swap` / 分片锁），性能不足再升级；
- 样本必须带时间戳，由 core 提供统一时钟源；
- **UI 读端 = 事件订阅**（core 变化 → emit → 前端 listen），**无轮询**；WebView 各自独立渲染，不共享事件循环（M1.2 关键改进，见 §5）。

## 5. 线程模型（M1.2：Tauri 主进程 + 独立 WebView 渲染）

```
┌─ pano-app 主进程（Rust，由 Tauri 事件循环驱动）─────────┐
│ · core 编排（适配器任务在 tokio 上运行）                │
│ · 窗口服务（Tauri WebviewWindow 生命周期 / 控制）        │
│ · UI 桥接：SampleStore 变化 → 事件 emit                 │
│ · 托盘（tray-icon，主进程侧）                           │
└───────────────────────────────────────────────────────┘
┌─ WebView 渲染进程（每个窗口独立，由操作系统调度）────────┐
│ · 前端组件渲染 / 交互；窗口间互不阻塞                    │
└───────────────────────────────────────────────────────┘
┌─ tokio runtime（multi-thread worker）─────────────────┐
│ · 每个启用适配器一个 task：采集循环                      │
│ · 远程适配器：HTTP 轮询 / WebSocket 长连接（§14 预留）    │
└───────────────────────────────────────────────────────┘
```

要点（含 M1.1 实测教训，见 roadmap §M1.1）：

- **窗口渲染互相独立**：egui/eframe 单事件循环 + 即时模式全量重绘，管理窗口持续高频渲染会挤占组件窗口重绘（实测组件窗口重绘空隙最长 16s）；Tauri 每个窗口独立 WebView 渲染进程，根治该问题；
- **数据驱动刷新 = 事件推送**：egui 空闲时不执行 UI 逻辑，帧内轮询会冻结（实测鼠标不动曲线冻结）；Tauri 前端靠事件驱动，无此问题；
- UI 主进程代码绝不执行阻塞 IO；适配器绝不在 UI 线程运行；
- 关闭顺序（M1.1 语义延续）：托盘「退出」→ 停止全部适配器 → 结束进程。

## 6. 注册表与特性开关

- `pano-adapters` 内部维护 `build() -> Vec<Box<dyn Adapter>>`，按 feature 条件收集已启用适配器；
- `pano-core` 的注册表建立 `AdapterId → AdapterHandle` 映射；
- id 重复 → `CoreError::DuplicateAdapter`，启动失败（开发期尽早暴露）。

## 7. UI 声明所需适配器（可替换 UI 的关键）

> 类型归属（M1.2 审查定稿，M2.2 扩展）：`Capability` / `UISpec` / `ComponentSpec` / `WindowSpec` 均为**纯数据声明**，定义在 `pano-core`（`capability.rs`）；`pano-window` 允许引用这些纯数据声明，但不依赖 core 的任何运行逻辑（见 §13）。

```rust
// M1.2 扩展：UISpec 增加组件清单（组件与窗口分离，R1）
pub struct UISpec {
    pub requires: Vec<Capability>,       // 本 UI 必须满足的能力
    pub components: Vec<ComponentSpec>,  // 组件目录（组件类型清单；窗口 = 实例，由播种/新建产生）
}

pub struct ComponentSpec {
    pub id: String,                      // 组件类型 id：全小写 ASCII、连字符分隔（如 "sys-cpu"）
    pub name: String,                    // M2.2：组件显示名（如 "CPU 使用率"）
    pub series: Vec<SeriesId>,           // 本组件固定消费的指标（窗口由此解析，不再直接存 series）
    pub window: WindowSpec,              // 默认窗口需求声明（纯数据）
}

pub struct WindowSpec {
    pub title: String,
    pub size: (f64, f64),                // 初始宽高（逻辑像素）
    pub position: Option<(f64, f64)>,    // 初始位置（缺省 = 居中 / 布局持久化恢复）
    pub always_on_top: bool,             // 初始置顶
    pub fullscreen: bool,                // 初始全屏
    pub monitor: Option<String>,         // 显示器偏好（序列化标识，如 "primary" / 自定义编号；缺省 = 主显示器 / 上次所在）
}
```

**UI 组件类型（M2.2）**：

- `UISpec.components` 即**组件目录**：每个 `ComponentSpec` 描述一种「窗口内容形态」——`id`（全小写 ASCII、连字符分隔）、`name`（显示名）、`series`（该组件**固定消费**的指标）、`window`（默认窗口规格）；
- **窗口 = 组件类型实例**：每个监控窗口绑定一个组件类型 id（`WindowEntry.component`），其展示的 series **由目录解析得出**（窗口不再直接存 series）；运行时注册表以窗口 id 为键，**多个窗口可绑定同一组件类型**；
- **切换组件 = 换内容**：对已开窗口 `set_window_component` 切换其组件类型（series 随之切换），命令层向该窗口 emit `pano://window-component` 事件，前端据此重载；窗口标题 / 几何不动；
- **组件目录可用性**：`list_components` 返回全部组件并附 `available`（全部 series 所属适配器已注册）；适配器未注册（feature 未编译）的组件置灰不可选，已注册但未启用的组件可选但窗口显示空态；
- **窗口集合持久化**（M2.2）：`[ui].windows` 段持久化监控窗口 id 列表。**段缺失**（`Option::None`）= 首次运行，按组件目录播种并写回；**段存在但列表为空** = 用户已销毁全部监控窗口，保持为空不重播种。`[window.<id>]` 段持久化 `component`（组件绑定）、`title`（窗口标题覆盖）与既有布局字段；运行时新建 / 切换 / 销毁窗口写回配置，重启恢复；
- 换组件 = 换渲染形态：前端按组件类型 id 分派渲染器（`renderers.ts`）；M2.2 通用渲染 = 组件 series 逐个「数值卡 + 曲线」；M2.3 实现专用渲染器 `SysDashboard.svelte`（注册 `sys-dashboard`，汽车仪表盘式）验证「换组件 = 换渲染」的可替换性。

**数据源增强（M2.3）**：专用组件可能消费适配器新 series / 配置：

- `sys.disk.active_percent`（磁盘活动率）= 当前**最忙磁盘**的真实忙碌时间 %（0..=100）：Windows 用 PDH `\PhysicalDisk(*)\% Disk Time` 逐盘采集、取非 `_Total` 实例最大值（真实忙碌时间，非字节速率；空闲时趋近 0，重负载趋近 100；统计范围为系统全部物理磁盘，`device` 过滤仅作用于容量系列）；配套 `sys.disk.busiest_disk`（Text，最忙盘盘符如 `C:`）供 UI 显示。**Windows 先行**，Linux / macOS 暂未实现（不产出该 series）；
- `sys.net.utilization`（链路利用率）=（recv_bps + sent_bps）/（参考带宽）× 100（钳 0..=100），参考带宽 `link_mbps` 可配（默认 1000 Mbps）；
- `high_threshold`（四个 sys 适配器通用配置，Number 默认 80，域 0..=100）：**高占用阈值**，供仪表盘指示灯判定。属「适配器自定义字段」：适配器自身不使用，仅作为数据源配置暴露；UI 经 `list_adapters` 返回的 `config`（当前自定义配置值）读取，未配置回落默认。

- **管理窗口**（1 个）不属于 `components`：由 pano-app 固定创建（内含适配器管理页 + 设置页），是 pano-ui 内置的固定窗口；
- **首次播种时机**：仅当组件的 `series` 存在数据源（对应适配器**已启用**）时才按目录播种组件窗口；无数据源组件不播种（在「窗口管理」页提示）。**用户新建窗口**可绑定已注册但未启用的组件（窗口内显示空态），仅对应适配器**未注册**（feature 未编译）的组件不可选；
- `WindowSpec.monitor` 在 core 侧为**字符串序列化形式**（`Option<String>`），由 pano-ui / pano-window 侧解析为 `MonitorId`（`MonitorId` 类型定义在 pano-window，属窗口领域，core 不引用）；布局持久化的 `[window.<id>]` 段同样存序列化形式。

`pano-app` 启动流程：

1. 按 feature 构建适配器注册表；
2. 加载配置（含 `[ui].windows` 窗口集合与 `[window.<id>]` 组件 / 布局持久化段），启用相应适配器；
3. **能力校验**：`UISpec.requires ⊆ 已启用适配器的能力并集`；不满足 → 明确报错（提示缺哪个适配器 / feature）退出；
4. 启动 core，装配窗口服务（pano-window），把 `CoreHandle`（只读 API）+ `WindowService` 交给 UI；
5. 固定创建管理窗口；监控窗口按 `[ui].windows` 持久化集合创建（缺省 = 首次运行，按组件目录播种并写回）；窗口服务按 `WindowSpec` 建窗，优先恢复 `[window.<id>]` 持久化布局。

效果：

- 换 UI = 新 UI crate 声明自己的 UISpec，core 校验通过即接入；
- 换窗口实现 = 换 pano-window 的实现（Tauri 版为默认），UI 组件不受影响（只持 `WindowHandle`）。

## 8. 配置与热重载

- 启动：`pano.toml` → 校验（schema_version、未知适配器 id、非法采样周期）→ 应用；
- 管理界面修改（启用/停用、采样间隔）→ **即时生效**（M1.1 已落地）：写回文件 → **仅重启受影响适配器**：
  - 停用：stop → Disabled；
  - 改采样周期：stop → 更新 → start；
- **自定义参数**编辑生效与失败回滚：归 M3（M1 遗留），M1.2 不扩展此范围；
- M1.2 新增 `[window.<id>]` 段：布局持久化（窗口位置 / 大小 / 所在显示器），由 pano-app 协调窗口服务读写（§13）；
- 热重载失败：回滚为原配置，UI 提示错误。

## 9. 错误与恢复策略

- 适配器采集异常：错误计数 + 退避重试（1s / 2s / 4s … 封顶 60s），超过阈值 → `Error` 状态并在 UI 标记；
- 远程数据源（§14）：连接失败 / 断线同样走退避重试，连接状态映射进 `AdapterStatus`；
- `Error` 状态的适配器可在管理界面手动 Restart；
- 平台不支持（如某指标在当前 OS 上不存在）→ 标记 `Unsupported`，不阻塞其他适配器；
- 窗口服务错误：`WindowError`（pano-window 库内 thiserror 定义，如 `WindowNotFound` / `MonitorUnavailable`），不崩溃进程；
- 任何适配器故障都不崩溃进程；core 记录最后错误信息供 UI 展示。

## 10. 可替换性验证路径（设计验收）

| 场景 | 操作 | 涉及改动 |
| --- | --- | --- |
| 换适配器 | 关 feature A、开 feature B，重编译 | `pano-adapters` 一处 |
| 新增适配器（含远程） | 实现 trait + 注册 + feature + 配置段 + 一致性测试 | 不动 core / ui / window |
| 换 UI | 新 crate 声明 UISpec（组件 + 窗口需求），用 core 只读 API 与 WindowHandle 重写前端 | 不动 core / adapters / window |
| 换窗口实现 | 换 pano-window 后端（如从 Tauri 换原生 winit 版） | 不动 core / adapters / ui 组件 |
| 增新页面/组件 | 在 pano-ui 内加组件（内容 + WindowSpec） | 不动其他 crate |

## 11. 目录结构（M1 已落地，M1.2 更新）

```
pano/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml           # 固定 stable 工具链
├── pano.toml.example             # 配置模板（pano.toml 本地配置不入库）
├── crates/
│   ├── pano-core/src/
│   │   ├── lib.rs
│   │   ├── adapter.rs            # Adapter trait、AdapterContext、状态、错误
│   │   ├── capability.rs         # Capability、UISpec、ComponentSpec
│   │   ├── error.rs              # CoreError
│   │   ├── registry.rs
│   │   ├── lifecycle.rs          # 启停、状态、能力校验、热生效
│   │   ├── sample_store.rs       # 环形缓冲
│   │   ├── config.rs             # pano.toml 解析 / 校验 / 序列化
│   │   └── test_harness.rs       # 适配器一致性测试基座
│   ├── pano-adapters/src/
│   │   ├── lib.rs                # build()：按 feature 收集
│   │   ├── example_counter.rs    # feature: adapter-example-counter
│   │   ├── example_sine.rs       # feature: adapter-example-sine
│   │   └── remote/               # 远程数据源共享基座（§14，预留）
│   │       ├── http_poll.rs      # HTTP 轮询模板
│   │       └── ws_push.rs        # WebSocket 推送模板
│   ├── pano-window/src/          # M1.2 新增：窗口服务
│   │   ├── lib.rs                # WindowService trait、WindowHandle、WindowError
│   │   ├── monitor.rs            # MonitorId、显示器枚举
│   │   ├── persist.rs            # 布局持久化（位置/显示器记忆）
│   │   └── tauri.rs              # Tauri v2 实现（WebviewWindow 封装）
│   │   # 注：WindowSpec / ComponentSpec 为纯数据声明，定义在 pano-core（§7），此处不重复定义
│   ├── pano-ui/                  # M1.2 重写：Tauri 前端 + Rust 命令层
│   │   ├── src/                  # Rust：tauri commands、事件桥接（core→前端）
│   │   ├── web/                  # 前端工程（Svelte + Vite，§ui.md §10）
│   │   │   ├── src/              # 组件内容实现（数值卡 / 曲线 / 管理页）
│   │   │   ├── src-tauri/        # Tauri 应用壳（或并入 pano-app）
│   │   │   └── ...
│   │   └── assets/               # 图标等静态资源
│   └── pano-app/src/main.rs      # 启动流程编排（core + window + ui / headless）
└── docs/
```

> 注：pano-window 与 pano-ui 的 Tauri 壳的具体归属（独立 `src-tauri` 还是并入 pano-app）在 M1.2 编码时按「依赖方向铁律」确定：窗口实现细节不得泄漏给 core / adapters。

## 12. 扩展点总结

- 新适配器三步接入：实现 trait → 加 feature 并注册 → 加配置段（+ 一致性测试）；远程适配器可复用 §14 基座；
- 新指标展示：UI 按 series id 订阅渲染即可，无需改适配器；
- 新窗口能力：在 pano-window 的 `WindowHandle` API 上扩展，UI 组件零改动；
- 预留插件化边界：trait 与 context 只传 `&dyn` / 不透明句柄、不泄漏内部类型，未来可平滑拆 cdylib 动态加载。

## 13. 窗口服务（pano-window，R1/R2 落地）

**动机**：UI 组件与窗口分离——组件只描述「画什么」（内容）与「想要什么样的窗口」（`WindowSpec`），窗口的创建、控制、持久化全部由主程序装配的窗口服务负责。UI 通过 API 调用实现全屏、置顶、绑定显示器等功能，不直接接触任何窗口实现。

职责：

- **窗口生命周期**：按 `WindowSpec` 创建窗口、挂载组件内容；聚焦 / 隐藏 / 关闭；
- **窗口控制 API**（`WindowHandle`，组件与主程序均可调用）：
  - `set_fullscreen(bool)` —— 全屏切换；
  - `set_always_on_top(bool)` —— 置顶切换；
  - `set_monitor(MonitorId)` —— 绑定特定显示器（配合位置/大小）；
  - `set_position(px, py)` / `set_size(w, h)` —— 窗口几何；
  - `focus()` / `show()` / `hide()` —— 可见性；
  - `close()` —— 关闭 = 隐藏（不退出进程，M1.1 语义延续；退出仅经托盘「退出」）。
- **布局持久化**：窗口关闭 / 退出时记录位置、大小、所在显示器到 `[window.<id>]` 配置段，下次启动恢复；也可由 API / 配置**指定显示器打开**；
- **显示器枚举**：`monitors() -> Vec<MonitorInfo>`，`MonitorId` 标识（主显示器 / 次显示器 / 自定义编号）。

依赖方向（M1.2 审查定稿）：

- `pano-window` 不依赖 core 的**运行逻辑**（lifecycle / sample_store / config 等），允许引用 core 的**纯数据声明**（`WindowSpec` / `ComponentSpec`，§7）；不依赖 adapters / ui；
- `pano-ui` 依赖 pano-window（仅取 `WindowHandle` / `MonitorId` 等类型）；
- `pano-app` 装配：创建 `WindowService` 实例（默认 Tauri v2 实现），按 `[ui].windows` 持久化窗口集合 / 组件目录播种建窗。

与 Tauri 的对应：

- 窗口 = `tauri::WebviewWindow`（独立 WebView 渲染进程）；
- 全屏 / 置顶 / 位置 / 大小 / 聚焦 / 显示器 = Tauri 原生窗口与 monitor API；
- **调用路径一律收敛**：前端经 Tauri 命令（`invoke`）→ Rust 命令层 → `WindowService` / `WindowHandle`；命令层与组件**不直接操作** Tauri 窗口类型（防止 Tauri 类型泄漏进 pano-ui，保证换窗口后端组件零改动）；
- **布局持久化写回**：窗口关闭 / 退出时，窗口服务把位置 / 大小 / 显示器数据交给 pano-app 装配层写 `[window.<id>]` 段；与 core 热重载写 pano.toml 由 pano-app 协调串行（段级合并，避免并发写）。

错误：`WindowError`（thiserror）：`WindowNotFound`、`MonitorUnavailable`、`AlreadyExists`、`Io` 等；库内不 panic。

## 14. 远程数据源预留（R3）

**动机**：适配器可能从远程获取数据——HTTP API 轮询（如 REST / Prometheus）或 WebSocket 推送（实时流）。M1.2 只做**能力预留与共享基座**，不实现具体远程适配器。

预留落点（已确认决策）：

1. **能力标记**：`Capability::RemoteSource`（标记类能力，UI 据此展示「远程数据源」标签）；
2. **core 注入点（轻量抽象，core 不依赖具体 HTTP 库）**：

```rust
// pano-core 定义（仅抽象，不引入 reqwest 等实现）
// 返回 boxed future 以保持 trait 可做 trait-object（dyn-compatible），零额外依赖
pub struct HttpResponse { pub status: u16, pub body: Vec<u8> }

#[derive(Debug, thiserror::Error)]
pub enum HttpError { /* Timeout / Status / Io 等，映射自具体实现 */ }

pub trait HttpClient: Send + Sync {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>>;
}
// AdapterContext 增加可选字段
pub struct AdapterContext {
    // ...既有字段
    pub http: Option<Arc<dyn HttpClient>>,  // pano-app 按 feature 装配（reqwest 实现）
}
```

3. **adapters 共享基座**（`pano-adapters/src/remote/`，具体依赖 reqwest / tokio-tungstenite 留在本层）：
   - `http_poll` 模板：按 `sampling` 周期请求 URL → 解析（serde_json）→ `sink.push`；请求失败**模板内置简易退避重试**（core 级自动退避归 M2，见 roadmap M1 遗留）；
   - `ws_push` 模板：`start` 建立长连接 → 消息解析 → `sink.push`；断线重连**模板内置简易退避**（同 http_poll）；连接状态映射 `AdapterStatus`（连接中 = `Starting`，断线重连 = `Error` + 计数，UI 可见）；
4. **凭据 / 请求参数**：走适配器自定义配置（`config_schema`），如 `url`、`headers`、`token`、`query` 等，不新增 core 机制。

设计约束：

- `Adapter` trait 与既有适配器零改动；新远程适配器只需在 `start` 里使用注入的 `HttpClient`（或自建连接）即可；
- core 的依赖面不膨胀（抽象在 core，实现与具体依赖在 adapters / pano-app 装配层）；
- 生命周期契约不变：`stop` 返回后不得再产出样本（WS 型需在 stop 中关闭连接并 join 接收任务）。
