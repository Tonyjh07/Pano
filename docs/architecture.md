# Pano 架构设计

## 1. 总体分层

```
┌────────────────────────────────────────────────────────┐
│  UI（pano-ui，egui）—— 可替换                          │
│   · 声明所需能力：UISpec                                │
│   · 只读访问 core 的样本/状态快照                       │
└───────────────────────┬────────────────────────────────┘
                        │ 公共只读 API（样本、状态、控制请求）
┌───────────────────────▼────────────────────────────────┐
│  CORE（pano-core）—— 不可替换的编排层                   │
│   · AdapterRegistry：AdapterId → 适配器实例             │
│   · Lifecycle：启停、状态机、错误恢复                   │
│   · SampleStore：按 series 组织的环形缓冲               │
│   · Config：加载 / 校验 / 应用 / 热重载                 │
│   · CapabilityMatcher：UI 声明 vs 已启用适配器能力       │
└───────────────────────┬────────────────────────────────┘
                        │ Adapter trait（唯一契约）
┌───────────────────────▼────────────────────────────────┐
│  ADAPTERS（pano-adapters）—— 可替换，特性开关编译        │
│   example.counter / example.sine / (M2) sys.*            │
└────────────────────────────────────────────────────────┘
```

核心不变量：

1. **适配器不知道 UI 存在**——它只向 SampleSink 推送样本；
2. **UI 不知道具体适配器**——它只按能力（Capability）和 series 取数据；
3. **替换任何一侧都不改动另一侧**。

## 2. 核心概念

| 概念 | 说明 |
| --- | --- |
| `Adapter` | 一个信息源，trait 契约见 §3 |
| `AdapterId` | 全局唯一，`<域>.<名称>` |
| `Capability` | 能力标签（如 `TimeSeries`、`SystemInfo`），UI 按能力声明需求 |
| `SeriesId` | 一条指标序列，`<adapter_id>.<metric>` |
| `Sample` | `{ timestamp, value }`；value 支持数值 / 布尔 / 文本 / JSON |
| `SampleSink` | 适配器持有的推送句柄（可 clone、多线程安全） |
| `SampleStore` | 核心持有的环形缓冲集合，UI 只读 |
| `AdapterStatus` | `Disabled / Stopped / Starting / Running / Error` |
| `UISpec` | UI 声明所需能力的清单（§7） |

## 3. Adapter trait（M1 定稿）

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
- `config: HashMap<String, ConfigValue>` —— 自定义配置（pano.toml 解析而来）。

`ConfigSchema` 只描述**自定义字段**；`enabled` / `sampling` 为 core 固定字段，不进 schema。

与草案的差异（M1 落地定稿）：

- 独立 `id()` 并入 `meta()`，避免两处 id 不一致；
- `AdapterContext` 增加 `runtime`（tokio Handle）与 `config`。

设计要点：

- 每个适配器**自持状态**，core 不假设其内部实现（单任务或多任务均可）；
- `start` 返回即视为已开始，适配器负责在 tokio 上自建任务循环；
- `stop` 必须干净退出（任务 join、资源释放）。

## 4. 数据流：推送 + 环形缓冲

```
适配器任务循环（每 sampling 一次）
  │  采集 / 计算
  ▼
SampleSink.push(Sample) ──► SampleStore 对应 series 的环形缓冲
                                │（写端：适配器任务）
                                ▼
UI 每帧 render()：读快照
  latest(series) / history(series, window)
  │（读端：egui 主线程）
  ▼
绘制数值卡、实时曲线
```

- 环形缓冲默认容量按 series 可配置（默认 4096 样本），满则覆盖最旧；
- 写端设计：M1 用 `RwLock`/`Mutex` 起步，预留替换点（`arc-swap` / 分片锁），性能不足再升级；
- 样本必须带时间戳，由 core 提供统一时钟源。

## 5. 线程模型

```
┌─ egui 主线程 ────────────────────────────┐
│ eframe 事件循环：render() 读快照、画 UI    │
│ 重绘由 core 按需触发（request_repaint）    │
└──────────────────────────────────────────┘
┌─ tokio runtime（multi-thread worker）────┐
│ 每个启用适配器一个 task：采集循环          │
│ push → 通知重绘（节流）                   │
└──────────────────────────────────────────┘
┌─ blocking pool（spawn_blocking）─────────┐
│ 阻塞式系统调用（读文件、syscall）          │
└──────────────────────────────────────────┘
```

- UI 线程绝不执行阻塞 IO；适配器绝不在 UI 线程运行；
- 样本推送后，若距上次重绘请求超过阈值（如采样间隔的一半）则 `request_repaint()`，使 UI 刷新率 ≈ 数据刷新率，不空转。

## 6. 注册表与特性开关

- `pano-adapters` 内部维护 `build() -> Vec<Box<dyn Adapter>>`，按 feature 条件收集已启用适配器；
- `pano-core` 的注册表建立 `AdapterId → AdapterHandle` 映射；
- id 重复 → `CoreError::DuplicateAdapter`，启动失败（开发期尽早暴露）。

## 7. UI 声明所需适配器（可替换 UI 的关键）

```rust
// 草案
pub struct UISpec {
    pub requires: Vec<Capability>,  // 本 UI 必须满足的能力
}
```

`pano-app` 启动流程：

1. 按 feature 构建适配器注册表；
2. 加载配置，启用相应适配器；
3. **能力校验**：`UISpec.requires ⊆ 已启用适配器的能力并集`；不满足 → 明确报错（提示缺哪个适配器 / feature）退出；
4. 启动 core，把 `CoreHandle`（只读 API）交给 UI。

效果：

- 换 UI = 新 UI crate 声明自己的 UISpec，core 校验通过即接入；
- 换适配器 = 改 feature 重新编译。

## 8. 配置与热重载

- 启动：`pano.toml` → 校验（schema_version、未知适配器 id、非法采样周期）→ 应用；
- 管理界面修改（启用/停用、采样间隔、自定义参数）→ 写回文件 → **仅重启受影响适配器**：
  - 停用：stop → Disabled；
  - 改采样周期 / 自定义参数：stop → 更新 → start；
- 热重载失败：回滚为原配置，UI 提示错误。

## 9. 错误与恢复策略

- 适配器采集异常：错误计数 + 退避重试（1s / 2s / 4s … 封顶 60s），超过阈值 → `Error` 状态并在 UI 标记；
- `Error` 状态的适配器可在管理界面手动 Restart；
- 平台不支持（如某指标在当前 OS 上不存在）→ 标记 `Unsupported`，不阻塞其他适配器；
- 任何适配器故障都不崩溃进程；core 记录最后错误信息供 UI 展示。

## 10. 可替换性验证路径（设计验收）

| 场景 | 操作 | 涉及改动 |
| --- | --- | --- |
| 换适配器 | 关 feature A、开 feature B，重编译 | `pano-adapters` 一处 |
| 新增适配器 | 实现 trait + 注册 + feature + 配置段 + 一致性测试 | 不动 core / ui |
| 换 UI | 新 crate 声明 UISpec，用 core 只读 API 重写界面 | 不动 core / adapters |
| 增新页面 | 在 pano-ui 内加页面组件 | 不动其他 crate |

## 11. 目录结构（M1 已落地）

```
pano/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml           # 固定 stable 工具链
├── pano.toml.example             # 配置模板（pano.toml 本地配置不入库）
├── crates/
│   ├── pano-core/src/
│   │   ├── lib.rs
│   │   ├── adapter.rs            # Adapter trait、AdapterContext、状态、错误
│   │   ├── capability.rs         # Capability、UISpec
│   │   ├── error.rs              # CoreError
│   │   ├── registry.rs
│   │   ├── lifecycle.rs          # 启停、状态、能力校验、热生效
│   │   ├── sample_store.rs       # 环形缓冲
│   │   ├── config.rs             # pano.toml 解析 / 校验 / 序列化
│   │   └── test_harness.rs       # 适配器一致性测试基座
│   ├── pano-adapters/src/
│   │   ├── lib.rs                # build()：按 feature 收集
│   │   ├── example_counter.rs    # feature: adapter-example-counter
│   │   └── example_sine.rs       # feature: adapter-example-sine
│   ├── pano-ui/src/
│   │   ├── lib.rs                # UISpec 声明 + build_app
│   │   ├── app.rs                # PanoApp（侧边栏 / 状态栏 / 重绘节流）
│   │   ├── theme.rs              # 暗色主题 + 中文字体
│   │   ├── pages/                # dashboard / adapters / settings
│   │   └── widgets/              # status_badge 等
│   └── pano-app/src/main.rs      # 启动流程编排（GUI / headless）
└── docs/
```

## 12. 扩展点总结

- 新适配器三步接入：实现 trait → 加 feature 并注册 → 加配置段（+ 一致性测试）；
- 新指标展示：UI 按 series id 订阅渲染即可，无需改适配器；
- 预留插件化边界：trait 与 context 只传 `&dyn` / 不透明句柄、不泄漏内部类型，未来可平滑拆 cdylib 动态加载。
