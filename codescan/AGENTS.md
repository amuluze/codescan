# codescan/AGENTS.md

CodeScan 工作区的本地工程约定。上位约束为仓库根 `../AGENTS.md` 与领域规范 `../.specs/domain/scan-engine.md`；冲突时以领域规范为准。

## 工作区边界

- 本目录是独立 Rust/pnpm 工作区；Cargo 入口为 `Cargo.toml`，Node 入口为 `package.json` 与 `pnpm-workspace.yaml`。
- `crates/engine/`：无 Tauri/UI 依赖的确定性引擎，全部能力可无头测试。
- `crates/cli/`：无头 CLI；`apps/desktop/`：React UI 与 Tauri 命令层。
- 不依赖仓库外站点源码或其构建产物。

## 代码约定

- Rust：`cargo clippy --workspace --all-targets -- -D warnings` 必须干净；公共 API 注释说明对应 Domain § 条款。
- `Result` 别名一律带默认错误参数：`type Result<T, E = StoreError> = std::result::Result<T, E>`。
- TypeScript：strict；类型导入用 `import type`；对象形状断言不在调用点散落，IPC 边界统一收口在 `apps/desktop/src/platform.ts`。
- 前端页面位于 `apps/desktop/src/views/`，跨页面复用组件位于 `apps/desktop/src/components/`，状态与数据事实位于 `stores/`。
- 确定性（Domain §2）是最高优先级：进入 Finding、ScanEvent、Stats 的字段不得携带时间、绝对路径或并行顺序等机器差异。耗时展示留在 UI 层，不入报告。
- 规则消息只表述「出现了构造」，不得声称可利用性、数据流可达性或跨过程证明。

## 命令入口

优先从仓库根使用 `task`；仅在需要直接过滤 Cargo 测试时进入本目录。

| 目的 | 命令 |
|---|---|
| 依赖安装 | 根：`task install` |
| 完整门禁 | 根：`task check` |
| TypeScript / Vitest / 前端构建 | 根：`task typecheck`、`task test`、`task build` |
| Rust 全工作区测试 | 根：`task test:rust` |
| Clippy | 根：`task check:rust` |
| CLI 黑盒测试 | 根：`task test:cli` |
| CodeScan 自扫描 | 根：`task scan`、`task scan:sarif` |

## 引擎与规则验证

在本目录内执行以下命令（Cargo 工作区根即 `codescan/`）：

```bash
# 规则改动：只运行相关前缀的 fixture 与规则
CODESCAN_FIXTURE_DIR=<前缀> CODESCAN_RULE_PREFIX=<前缀> \
  cargo test -p codescan-engine --test rule_suite

# 扫描范围 / 忽略源改动
cargo test -p codescan-engine --test integration ignore_sources_and_defaults
```

规则 fixture 位于 `crates/engine/tests/fixtures/rules/<rule-id>/`；内置 YAML 只放 `crates/engine/rules/`。fixture 语义与标注格式见 `docs/rules.md`，中英文文档必须同步。

新语言接入必须同时包含：`crates/engine/src/language.rs` 枚举与语法、`metrics.rs` 语法档位、至少 6 条规则与对应 fixture、`integration.rs` 语言矩阵探针。

## 文档与发布

- 用户可见命令、行为与边界变化同步更新 `README.md`、`README.zh-CN.md` 与 `docs/`；英文对应文件在 `docs/en/`。
- `DESIGN.md` 是桌面端设计意图的 SSOT；改动 UI 结构与组件落点时同步更新。
- 发布命令从仓库根执行：`task build:dmg`。仅支持 macOS Apple Silicon（arm64）；产物在 `codescan/target/aarch64-apple-darwin/release/bundle/`。
- `apps/desktop/src-tauri/gen/`、`apps/desktop/src-tauri/tauri.ci.conf.json`、`target/`、`artifacts/` 均为生成物，不提交。
- 新增任务先在 `codescan/package.json` 实现，再由根 `package.json` 转发并暴露为 `Taskfile.yml` 任务。
