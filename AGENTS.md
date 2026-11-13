# AGENTS.md

本仓库的跨产品工程约定。开始任务时按顺序阅读：

1. 本文件：产品边界、SDD 流程、全仓验证路由。
2. 目标产品目录的约定：`codescan/AGENTS.md` 或 `website/AGENTS.md`。
3. 与改动相关的领域规范：`.specs/domain/*.md`。

## 产品边界

| 路径 | 职责 | 本地约定 |
|---|---|---|
| `codescan/` | CodeScan Rust/pnpm 工作区：引擎、CLI、Tauri 桌面应用 | `codescan/AGENTS.md` |
| `website/` | 独立静态官网，无包管理与构建步骤 | `website/AGENTS.md` |
| `.specs/`、`.plans/` | 跨产品领域规范、任务规格与实施计划 | 本文件 |
| `.github/`、`package.json`、`pnpm-workspace.yaml`、`Taskfile.yml` | 共享 CI 与任务编排 | 本文件 |

- 产品目录拥有各自的实现、依赖与本地约定；产品专属构建命令和代码规范留在产品目录，跨产品规格仍集中在 `.specs/`。
- `website/` 不共享 CodeScan 的 Node/Rust 工作区，也不得依赖其构建产物。
- 跨产品改动必须在 Task Spec 中列出涉及产品及各自验证入口。

## 工作流

采用 SDD（Spec-Driven Development）：

- `.specs/domain/`：领域不变量，长期演进，最高约束来源。
- `.specs/tasks/`：任务规格，验收必须可客观判定；完成时 frontmatter 置 `status: done`。
- `.plans/`：实现路径，不得反向改写 Spec 验收。
- `reports/`：调研记录，不作为实现依据。

长期不变量变化先修订 Domain Spec，再写 Task Spec，最后实现。实现与文档冲突时，以相关 Domain Spec 为准，并同步修正实现或文档，不保留两套口径。

## 通用约束

- 不在仓库根添加应用依赖、产品实现或兼容转发层。根 `pnpm-workspace.yaml` 仅配置编排包、不包含工作区成员；CodeScan 的工作区配置只在 `codescan/pnpm-workspace.yaml`。
- 不提交生成物、构建产物、凭据或本机配置；各产品目录的忽略规则必须覆盖自身产物。
- 产品能力与数量声明以实现为事实来源，文档不得提前声称未实现能力。
- 命令实现只有一份权威来源：CodeScan 在 `codescan/package.json`；根 `package.json` 只转发，`Taskfile.yml` 只编排。
- 行为变更以最小、针对性的证据证明；不为走查重跑无关全套测试。

## 命令与验证路由

任务从仓库根执行；`task --list` 查看全部入口。

| 改动范围 | 验证入口 |
|---|---|
| CodeScan 代码、规则、CLI、桌面端 | 遵循 `codescan/AGENTS.md`；提交前 `task check` |
| 官网 HTML/CSS/JS | 遵循 `website/AGENTS.md`；静态站点无构建门禁，按该目录说明本地预览 |
| 跨产品文档、规格、CI、根编排 | 校验受影响路径与命令；涉及 CodeScan 代码路径时运行 `task check` |

仅 CodeScan 存在打包流程：`task build:dmg`，且仅支持 macOS Apple Silicon（arm64）。真实发布、签名与公证不属于常规验证，除非用户明确要求。
