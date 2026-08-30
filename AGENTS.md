# AGENTS.md — Pano 开发与测试规范（代理工作指南）

> 本文是 Pano 项目中**人类开发者与 AI 编码代理共同遵守**的核心开发与测试规范。
> 详细设计见 [`docs/`](docs/)：规范设计 `docs/spec.md`、架构设计 `docs/architecture.md`、图形界面设计 `docs/ui.md`、里程碑 `docs/roadmap.md`。

## 1. 项目概览

- **Pano（Panorama）**：Rust + egui 的可插拔监控面板。
- 核心架构：**适配器（可替换，获取信息）→ 核心 core（编排）→ UI（可替换，声明所需适配器）**。
- 关键决策：trait 注册表 + 编译期特性开关；推送 + 环形缓冲；tokio 运行时。

## 2. 核心开发规范

### 2.1 crate 划分与依赖方向（铁律）

| crate | 职责 | 依赖方向 |
| --- | --- | --- |
| `pano-core` | 适配器注册表、生命周期、样本存储（环形缓冲）、配置、能力匹配 | 不依赖 UI 与任何具体适配器 |
| `pano-adapters` | 内置适配器集合，每适配器一个 feature | 只实现 core 定义的 trait |
| `pano-ui` | egui 界面：页面、组件、主题；声明 UISpec | 只读 core 的公共 API |
| `pano-app` | 二进制入口：组装 core + ui | 依赖以上三者 |

**铁律**：core 不知道具体适配器和 UI；UI 不知道具体适配器；适配器不知道 UI。
任何改动都不得破坏这一依赖方向。

### 2.2 命名规范

- adapter id：`<域>.<名称>`（全小写 ASCII、连字符分隔），如 `example.counter`、`sys.cpu`
- series id：`<adapter_id>.<指标>`，如 `example.counter.value`
- capability：上驼峰语义化，如 `TimeSeries`、`SystemInfo`
- crate：`pano-<领域>`（如 pano-core、pano-ui）；trait：上驼峰名词；模块：小写下划线

### 2.3 特性开关

- 每个内置适配器一个 feature：`adapter-<名称>`
- 换适配器 = 改 feature 重新编译；未启用 feature 的适配器不参与编译
- 默认特性与 `pano-app` 对 feature 的透传遵循 spec §5（M1 默认示例适配器；M2 起默认系统监控适配器）

### 2.4 错误处理

- 库 crate（core / adapters / ui）用 `thiserror` 定义错误枚举，**不 panic**；应用层用 `anyhow`
- 适配器运行期错误**不得**崩溃进程：转为 `AdapterStatus::Error` + tracing 日志（退避重试，见架构 §9）
- 错误分类遵循 `docs/spec.md` §6（`AdapterError::*`、`CoreError::*`）

### 2.5 日志

- 统一 `tracing`；target：`pano::core`、`pano::adapters::<模块名>`（模块名小写下划线，如 `pano::adapters::example_counter`）、`pano::ui`
- 级别约定：`trace` 样本级 / `debug` 生命周期 / `info` 关键事件 / `warn` 可恢复失败 / `error` 故障
- 库内禁止 `println!` / `eprintln!`

### 2.6 配置

- `pano.toml`：`schema_version`、`[core]`、`[adapters.<id>]`（固定字段 `enabled`、`sampling`，其余为该适配器自定义字段）
- 热重载：修改配置 → 写回文件 → **仅重启受影响适配器**；失败回滚原配置

## 3. 测试规范

- **单元测试**：覆盖核心逻辑（环形缓冲、注册表、配置解析）。
- **适配器一致性测试（必需）**：每个适配器必须通过 `pano-core` 测试基座：
  1. 生命周期：start → 产出样本 → stop 干净退出；
  2. 采样周期正确（容差内）；
  3. 非法配置返回 `AdapterError::Config`。
- UI 测试：M2 起引入 `egui_kittest` 无头交互测试。
- **提交门禁**：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test` 全部通过后方可提交；M2 起随 CI 接入 `cargo audit`（见 spec §9）。

## 4. 提交工作流（Git）

### 4.1 提交前审查（必须）

每次提交前，必须调用**可复用审查 Subagent**（Pano 审查员）进行审查：

1. 将待提交变更（`git diff --cached`）与本次修改摘要交给审查 Subagent；
2. 审查员基于本规范与 `docs/` 设计文档审查，输出：
   - **阻断（blockers）**：违反规范、破坏架构铁律、会导致构建/测试失败的问题；
   - **建议（suggestions）**：可优化但不影响正确性的改进点；
   - **结论**：`APPROVE`（可提交）/ `REQUEST_CHANGES`（需修复）。
3. 处理规则：
   - 存在**阻断**：必须修复；修复后**复用同一 Subagent 再次审查**，直至无阻断；
   - 仅有**建议**：确认建议不影响正确性、且无未决阻断后，**可直接提交**（建议可记录为后续 TODO，不阻塞本次提交）；
   - 审查 Subagent 保持**可复用**（同一会话延续），保证审查标准一致。
4. 每个提交对应一个审查Subagent，提交后删除，不得在不同提交上复用。

### 4.2 提交要求

- 提交信息格式：`<type>: <摘要>`，type ∈ `docs / feat / fix / refactor / test / chore`
- 提交前确保 §3 门禁通过；只提交本次主题相关文件

### 4.3 合并分支（必须用户确认）

**合并分支（如 dev → main）必须由用户实测并明确同意后才能执行**：

1. 用户运行/实测待合并版本，确认功能正常；
2. 用户明确表示同意合并后，才允许执行 merge；
3. 合并后按 spec §11 维护 `CHANGELOG.md` 并打 tag（`v0.x.y`）。

## 5. 变更流程

1. **设计先行**：涉及架构/规范的变更，先更新 `docs/` 再动代码；
2. **同步交付**：更新代码时，须一并包含对应的测试代码与文档更新（`docs/`、README 等），不得只提交代码而不带测试与文档；
3. **小步提交**：每次提交聚焦一个主题；
4. 提交前走 §4.1 审查流程；合并前走 §4.3 用户确认流程。
