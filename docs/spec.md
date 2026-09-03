# Pano 规范设计（工程规范）

本文定义 Pano 项目的**工程规范**：技术选型、workspace 组织、命名、错误、日志、配置、测试、依赖与发布约定。

规范服务于「三层可替换架构」的长期演进，优先保证三点：**边界清晰、替换低成本、新手可上手**。

## 1. 项目定位

- 主要用途：监控面板（实时数据采集与展示）。
- 可扩展：不限于监控，任何「信息采集 + 展示」的场景。
- 形态：桌面应用、单进程、跨平台（Windows / Linux / macOS）。

## 2. 技术选型

| 领域 | 选择 | 理由 |
| --- | --- | --- |
| 语言 | Rust（edition 2021，MSRV 跟随当前 stable） | 性能、类型安全、无 GC |
| GUI 框架 | **Tauri v2**（Rust 主进程 + 系统 WebView） | 多窗口独立渲染进程、托盘/置顶/全屏/多显示器生态成熟；替代 egui（M1.1 实测：egui/eframe 单事件循环 + 即时模式全量重绘，多窗口高频交互相互挤占，见 `roadmap.md` §M1.1） |
| 前端 | **Svelte 5 + Vite + TypeScript**（pano-ui 内部实现，可替换） | 轻量、反应式；Vite 为 Tauri 官方标配 |
| 图表 | **uPlot**（备选 ECharts） | 时间序列极快，适合实时曲线（egui_plot 随 egui 迁移不再使用） |
| 异步 | tokio（multi-thread worker） | 采集任务并发；后续 HTTP 轮询 / WebSocket 类适配器直接受益 |
| 配置 | serde + toml | 人类可读、生态成熟 |
| 日志 | tracing + tracing-subscriber | 结构化、按模块过滤 |
| 错误 | thiserror（库）+ anyhow（应用层） | 规范、低成本 |
| 时间 | 统一时钟源（M1 定 chrono 或 time） | 采样时间戳与曲线对齐 |

> 依赖策略见 §9：初版保持最小依赖集合。

## 3. Workspace 与 crate 划分

单一 workspace，五个 crate：

| crate | 类型 | 职责 | 依赖方向 |
| --- | --- | --- | --- |
| `pano-core` | lib | 适配器注册表、生命周期、样本存储（环形缓冲）、配置管理、能力匹配 | 仅依赖基础库（tokio/serde 等） |
| `pano-adapters` | lib | 内置适配器集合；每个适配器一个 feature；远程数据源共享基座 + 远程适配器宿主（M4，architecture §14.1） | 依赖 pano-core 的 trait 与 context |
| `pano-window` | lib | 窗口服务：窗口生命周期、控制 API（全屏/置顶/显示器绑定/位置）、布局持久化 | 不依赖 core 运行逻辑（可引用 core 的纯数据声明）、不依赖 adapters / ui |
| `pano-ui` | lib | Tauri 前端 + Rust 命令层：组件内容、事件桥接（core→前端）、窗口 API 调用 | 依赖 pano-core 只读 API + pano-window 类型 |
| `pano-app` | bin | 组装：构建注册表 → 加载配置 → 启动 core → 装配窗口服务 → 挂载 ui | 依赖以上四者 |

依赖方向铁律（可替换性的基础）：

- `pano-core` 不知道任何具体适配器和 UI；
- `pano-ui` 只知道 core 的公共只读 API 与 pano-window 的窗口 API，不知道具体适配器；
- `pano-adapters` 不知道 UI / 窗口，只实现 core 定义的 trait；
- `pano-window` 不依赖 core 的**运行逻辑**，允许引用 core 的**纯数据声明**（`WindowSpec` / `ComponentSpec`，架构 §7）；不依赖 adapters / ui；由 pano-app 装配。

## 4. 命名规范

| 对象 | 规范 | 示例 |
| --- | --- | --- |
| crate | `pano-<领域>` | pano-core / pano-ui / pano-window |
| adapter id | `<域>.<名称>`，全小写 ASCII，连字符分隔 | `example.counter` / `sys.cpu` |
| series id | `<adapter_id>.<指标>` | `example.counter.value` |
| component id（组件类型 id，即监控窗口 label 源） | 全小写 ASCII，连字符分隔，全局唯一 | `sys-cpu` |
| capability | 上驼峰，语义化 | `TimeSeries` / `SystemInfo` / `RemoteSource` |
| trait | 上驼峰名词 | `Adapter` / `SampleSink` / `WindowService` |
| 模块 | 小写下划线 | `registry` / `sample_store` / `window` |

## 5. 特性开关规范（feature flags）

- 每个内置适配器一个 feature：`adapter-example-counter`、`adapter-example-sine`……
- 默认特性：M1 为全部示例适配器；M2 起默认只含系统监控适配器。
- `pano-app` 透传 feature 到 `pano-adapters`；用户通过 `cargo build --features ...` 决定打进哪些适配器。
- 未启用 feature 的适配器**不参与编译**——「换适配器」= 改 feature 重新编译；
- **M4 起远程适配器不经 feature**：外部服务由 `[adapters."<id>"].remote` 配置 + 管理面板主动连接驱动（architecture §14.1），进程内以**远程适配器宿主**实例化注册，core / UI 无感本地 / 远程；cdylib 动态加载方案已放弃（roadmap M4）。

## 6. 错误处理规范

- 库 crate（core / adapters / ui）用 `thiserror` 定义错误枚举，**不 panic**。
- 应用层（pano-app）用 `anyhow` 做上下文包装。
- 错误分类（草案）：

| 错误 | 含义 |
| --- | --- |
| `AdapterError::NotAvailable` | 数据源/资源不可用 |
| `AdapterError::Unsupported` | 当前平台不支持该能力 |
| `AdapterError::Timeout` | 采集超时 |
| `AdapterError::Config` | 配置非法 |
| `AdapterError::Io(io::Error)` | IO 底层错误 |
| `HttpError` | 远程数据源请求错误（定义在 pano-core，映射自具体 HTTP 实现） |
| `CoreError::UnknownAdapter` | 引用不存在的适配器 |
| `CoreError::AlreadyRunning` / `NotRunning` | 生命周期非法操作 |
| `CoreError::DuplicateAdapter` | 适配器 id 重复（启动失败） |
| `CoreError::CapabilityUnsatisfied` | UI 声明的能力无法满足（启动失败） |
| `WindowError` | 窗口服务错误（定义在 pano-window）：`WindowNotFound` / `MonitorUnavailable` / `AlreadyExists` / `Io` 等 |

- 适配器运行中的错误**不得**让进程崩溃：转为状态 + tracing 日志（见架构 §9 恢复策略）；
- **远程适配器（M4）连接错误**由宿主映射进 `AdapterStatus`（连接中 = `Starting`，断线 = `Error` + 退避重连，architecture §14.1），不新增错误枚举；外部服务进程崩溃不影响 core 与其他适配器。

## 7. 日志规范

- 统一 `tracing`；target 命名：`pano::core`、`pano::adapters::<模块名>`（如 `pano::adapters::example_counter`）、`pano::ui`、`pano::window`。
- 级别约定：
  - `trace`：样本级调试（默认关闭）
  - `debug`：生命周期切换、配置应用
  - `info`：启动/停止、关键事件
  - `warn`：可恢复的采集失败
  - `error`：适配器进入 Error 状态、核心故障
- 禁止在库内使用 `println!` / `eprintln!`。

## 8. 配置规范

- 运行时配置：`pano.toml`（放用户配置目录，M1 定路径；开发期可放项目根目录）。
- 顶层字段：`schema_version`（当前 1）、`[core]`（默认采样策略等）、`[adapters.<id>]`、`[ui]`（M2.2：持久化监控窗口 id 列表 `windows`）、`[window.<id>]`（窗口持久化：`component` 组件绑定 + `title` 标题覆盖 + 布局 `position` / `size` / `monitor`，由命令层 / 窗口服务读写）。
- 每个适配器配置段固定字段：`enabled`、`sampling`（如 `"500ms"`），其余为该适配器自定义字段（**发给外部服务的 `/start` 配置**，含 `sampling`；本地适配器即其采集参数）。
- **M4 远程适配器段**：`[adapters."<id>"]` 增 `remote`（`endpoint` 必填、`token` 可选 → `Authorization` / 自定义头）——**Pano 侧连接信息，不进 `/start` 请求体**，与自定义字段（进 `/start`）区分；连接由管理面板「连接远程适配器」发起并写回，重启自动恢复（architecture §14.1）；
- **M4 UI 插件目录**：`ui-plugins/<plugin-id>/`（`manifest.json` + `dist/`），启动扫描合并进组件目录（architecture §15）；无该目录 → 行为与现状一致。
- 配置加载失败 → 启动报错并明确提示，不静默。
- **热重载原则**：管理界面修改配置 → 写回文件 → 仅重启受影响的适配器（M1 实现启用/停用与采样间隔；复杂参数热重载 M3 完善）。

## 9. 依赖管理规范

- 新增依赖需满足：必要性、维护活跃度、传递依赖可控。
- CI 中执行 `cargo audit`（M2 起）。
- 第三方依赖锁定版本，升级走 PR + changelog 记录。
- **框架依赖分层**：Tauri（pano-window / pano-app 装配层；pano-ui 的 Rust 命令层允许依赖 tauri 的**命令与事件 API**，但不依赖窗口实现，窗口控制一律经 `WindowHandle`）、HTTP / WebSocket 客户端（reqwest / tokio-tungstenite，仅 pano-adapters 的 remote 基座使用）不得进入 pano-core；前端依赖（Svelte / Vite / uPlot）仅存在于 `pano-ui/web/`，经 `package.json` / lockfile 管理。

## 10. 测试规范

- 单元测试：每个 crate 内，覆盖核心逻辑（环形缓冲、注册表、配置解析、窗口布局持久化）。
- **适配器一致性测试**（关键）：pano-core 提供测试基座（fake context），每个适配器必须通过：
  1. 生命周期：start → 产出样本 → stop 干净退出；
  2. 采样周期正确（容差内）；
  3. 非法配置返回 `AdapterError::Config`。
  - **远程适配器宿主（M4）例外**：样本经 WS 异步推送，「采样周期正确」判定不适用——以「首样本到达 + stop 后无样本」判定（architecture §14.1，loopback 假外部服务）。
- UI 测试：前端单测（Vitest，组件渲染与交互）+ Tauri 命令层单测（Rust）；端到端（Playwright / tauri-driver）M2 起视需要引入。
- 门禁：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`（M2 起接入 CI）。

## 11. 版本与发布

- semver；`CHANGELOG.md` 维护；tag 命名 `v0.x.y`。
- 0.x 阶段允许快速演进，但公开 trait 的破坏性变更必须在 changelog 注明。

## 12. 工具链

- `rust-toolchain.toml` 固定工具链（stable，跟随发布）。
- rustfmt 默认配置；clippy `all` 全开并 `-D warnings`。
