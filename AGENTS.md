# AGENTS.md

本仓库的工程约定。新会话先读本文件与 `.specs/domain/scan-engine.md`。

## 工作流

SDD（Spec-Driven Development），约定与 axiom 仓库一致（参照 `axiom/.specs/domain/spec-authoring.md`）：

- `.specs/domain/`：领域不变量，长期演进，最高约束来源。
- `.specs/tasks/`：任务规格，验收必须可客观判定；完成时 frontmatter `status: done`。
- `.plans/`：实现路径（怎么建），不得反向改写 Spec 验收。
- `reports/`：调研记录。

## 代码约定

- Rust：`cargo clippy --workspace --all-targets -- -D warnings` 干净；公共 API 带注释说明不变量来源（Domain § 条款）。
- `Result` 别名一律带默认错误参数：`type Result<T, E = StoreError> = std::result::Result<T, E>`。
- TS：strict；类型导入用 `import type`；不做对象形状的内联断言访问（IPC 边界在 platform.ts 一处收口）。
- 确定性（Domain §2）是最高优先级：任何进入 Finding/ScanEvent/Stats 的字段不得携带机器差异（时间、路径形态、并行顺序）。UI 层的耗时展示在 UI 层测，不入报告。

## 引擎与规则

- 引擎改动必须跑：`npm run test:rust`（含规则套件、确定性、fail-closed、SARIF、性能）。
- 规则改动必须跑对应前缀的规则套件：`CODESCAN_FIXTURE_DIR=<前缀> CODESCAN_RULE_PREFIX=<前缀> cargo test -p codescan-engine --test rule_suite`。
- 规则消息只表述「出现了构造」，不得声称可利用性或数据流可达（v1 无污点分析）。
- 新语言接入：`language.rs` 加枚举与语法 + 语法档位（metrics.rs Profile）+ 至少 6 条规则与 fixture + 语言矩阵探针（integration.rs）。

## 验证习惯

- 提交前：`npm run check:rust` + `npm run test:rust` + `npm run typecheck` + `npm run test` + `npm run build`。
- CLI 改动：`cargo test -p codescan-cli`（黑盒退出码/格式/基线语义）。
- 扫描范围/忽略改动：`cargo test -p codescan-engine --test integration ignore_sources_and_defaults`，并保持 Domain §3.1 的忽略源枚举同步。
- 打包：`pnpm --filter @codescan/desktop tauri build`（本地验证 .app/.dmg；CI 用 release workflow，需 tag）。
- 行为变更以最小测试证明，不为走查而重跑全套。
