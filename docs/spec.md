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
| GUI | egui / eframe（官方即时模式框架） | 纯 Rust、跨平台、开发迭代快；即时模式天然适合高频刷新面板 |
| 图表 | egui_plot | egui 官方配套，曲线/直方图 |
| 异步 | tokio（multi-thread worker） | 采集任务并发；后续 HTTP 轮询类适配器直接受益 |
| 配置 | serde + toml | 人类可读、生态成熟 |
| 日志 | tracing + tracing-subscriber | 结构化、按模块过滤 |
| 错误 | thiserror（库）+ anyhow（应用层） | 规范、低成本 |
| 时间 | 统一时钟源（M1 定 chrono 或 time） | 采样时间戳与曲线对齐 |

> 依赖策略见 §9：初版保持最小依赖集合。

## 3. Workspace 与 crate 划分

单一 workspace，四个 crate：

| crate | 类型 | 职责 | 依赖方向 |
| --- | --- | --- | --- |
| `pano-core` | lib | 适配器注册表、生命周期、样本存储（环形缓冲）、配置管理、能力匹配 | 仅依赖基础库（tokio/serde 等） |
| `pano-adapters` | lib | 内置适配器集合；每个适配器一个 feature | 依赖 pano-core 的 trait 与 context |
| `pano-ui` | lib | egui 界面：页面、组件、主题；**声明所需适配器能力** | 依赖 pano-core 的只读 API |
| `pano-app` | bin | 组装：构建注册表 → 加载配置 → 启动 core → 挂载 ui | 依赖以上三者 |

依赖方向铁律（可替换性的基础）：

- `pano-core` 不知道任何具体适配器和 UI；
- `pano-ui` 只知道 core 的公共只读 API，不知道具体适配器；
- `pano-adapters` 不知道 UI，只实现 core 定义的 trait。

## 4. 命名规范

| 对象 | 规范 | 示例 |
| --- | --- | --- |
| crate | `pano-<领域>` | pano-core / pano-ui |
| adapter id | `<域>.<名称>`，全小写 ASCII，连字符分隔 | `example.counter` / `sys.cpu` |
| series id | `<adapter_id>.<指标>` | `example.counter.value` |
| capability | 上驼峰，语义化 | `TimeSeries` / `SystemInfo` / `SystemMetrics` |
| trait | 上驼峰名词 | `Adapter` / `SampleSink` |
| 模块 | 小写下划线 | `registry` / `sample_store` |

## 5. 特性开关规范（feature flags）

- 每个内置适配器一个 feature：`adapter-example-counter`、`adapter-example-sine`……
- 默认特性：M1 为全部示例适配器；M2 起默认只含系统监控适配器。
- `pano-app` 透传 feature 到 `pano-adapters`；用户通过 `cargo build --features ...` 决定打进哪些适配器。
- 未启用 feature 的适配器**不参与编译**——「换适配器」= 改 feature 重新编译。

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
| `CoreError::UnknownAdapter` | 引用不存在的适配器 |
| `CoreError::AlreadyRunning` / `NotRunning` | 生命周期非法操作 |
| `CoreError::DuplicateAdapter` | 适配器 id 重复（启动失败） |
| `CoreError::CapabilityUnsatisfied` | UI 声明的能力无法满足（启动失败） |

- 适配器运行中的错误**不得**让进程崩溃：转为状态 + tracing 日志（见架构 §9 恢复策略）。

## 7. 日志规范

- 统一 `tracing`；target 命名：`pano::core`、`pano::adapters::example_counter`、`pano::ui`。
- 级别约定：
  - `trace`：样本级调试（默认关闭）
  - `debug`：生命周期切换、配置应用
  - `info`：启动/停止、关键事件
  - `warn`：可恢复的采集失败
  - `error`：适配器进入 Error 状态、核心故障
- 禁止在库内使用 `println!` / `eprintln!`。

## 8. 配置规范

- 运行时配置：`pano.toml`（放用户配置目录，M1 定路径；开发期可放项目根目录）。
- 顶层字段：`schema_version`（当前 1）、`[core]`（默认采样策略等）、`[adapters.<id>]`。
- 每个适配器配置段固定字段：`enabled`、`sampling`（如 `"500ms"`），其余为该适配器自定义字段。
- 配置加载失败 → 启动报错并明确提示，不静默。
- **热重载原则**：管理界面修改配置 → 写回文件 → 仅重启受影响的适配器（M1 实现启用/停用与采样间隔；复杂参数热重载 M3 完善）。

## 9. 依赖管理规范

- 新增依赖需满足：必要性、维护活跃度、传递依赖可控。
- CI 中执行 `cargo audit`（M2 起）。
- 第三方依赖锁定版本，升级走 PR + changelog 记录。

## 10. 测试规范

- 单元测试：每个 crate 内，覆盖核心逻辑（环形缓冲、注册表、配置解析）。
- **适配器一致性测试**（关键）：pano-core 提供测试基座（fake context），每个适配器必须通过：
  1. 生命周期：start → 产出样本 → stop 干净退出；
  2. 采样周期正确（容差内）；
  3. 非法配置返回 `AdapterError::Config`。
- UI 测试：M2 起引入 `egui_kittest` 做无头交互测试。
- 门禁：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`（M2 起接入 CI）。

## 11. 版本与发布

- semver；`CHANGELOG.md` 维护；tag 命名 `v0.x.y`。
- 0.x 阶段允许快速演进，但公开 trait 的破坏性变更必须在 changelog 注明。

## 12. 工具链

- `rust-toolchain.toml` 固定工具链（stable，跟随发布）。
- rustfmt 默认配置；clippy `all` 全开并 `-D warnings`。
