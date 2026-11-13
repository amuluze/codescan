---
status: done
---

# Task Spec：CodeScan 与官网分根及 CodeScan 目录分层整理

> 2026-11-13 更新：官网源码已迁出公开仓，改由私有 `amuluze/codescan-pro` 维护；本规格中 `website/` 同级边界及 T6、T8 的官网相关验收成为历史记录，CodeScan 目录分层验收仍有效。

task-id: `codescan-repository-layout`

## 目标

将 CodeScan 的完整 Rust/pnpm 工作区迁入 `codescan/`，与独立静态官网 `website/` 形成同级产品边界；仓库根只保留跨产品治理、任务编排和共享规格。同步整理 CodeScan 前端页面目录与引擎规则夹具目录，使目录名称直接表达资产职责。

## 范围

- 将 CodeScan 自有的 `apps/desktop/`、`crates/`、`assets/`、`docs/`、`examples/`、`scripts/`、`.codescanignore`、`CHANGELOG.md`、双语产品 README、`DESIGN.md` 及 Rust/pnpm 工作区配置和锁文件迁入 `codescan/`。
- 仓库根新增轻量 `package.json` 编排入口；根 `Taskfile.yml` 继续提供既有任务名，实际命令由 `codescan/package.json` 实现。根 README 使用英文并提供 CodeScan 中文文档链接。
- CodeScan 前端页面组件迁入 `apps/desktop/src/views/`；`src/components/` 只保留跨页面复用组件。
- 内置规则 YAML 保持在 `crates/engine/rules/`；规则测试夹具迁入 `crates/engine/tests/fixtures/rules/`，测试 runner 与贡献文档同步。
- 更新 GitHub Release workflow、Dependabot、忽略规则、脚本工作目录、构建产物路径和文档链接。
- 官网仍是无依赖静态站点；仅同步其对 CodeScan 源码与品牌/截图源资产的路径说明，不改变页面行为。

## 验收标准

- **T1 产品分根**：`codescan/Cargo.toml` 与 `codescan/pnpm-workspace.yaml` 是 CodeScan 工作区入口；`website/` 与 `codescan/` 同级；`codescan/Cargo.lock` 与 `codescan/pnpm-lock.yaml` 是唯一锁文件，仓库根不保留二者。根包名为 `codescan-workspace`、不声明应用版本或应用依赖；除 `setup` 以 `pnpm --dir codescan install` 安装依赖外，根 scripts 均为 `pnpm --dir codescan run <script>` 转发，`codescan/package.json` 是命令实现唯一权威源。
- **T2 命令兼容**：根 `task --list` 可枚举既有任务，Taskfile 不直接复制 CodeScan 命令实现。`task install` 后依赖安装到 `codescan/` 工作区；根 `task build`、`task scan` 实际执行成功；根 `task dev` 能启动 Vite 就绪监听；`task check` 与 `task test:cli` 由 T7 验证。
- **T3 前端职责**：五个页面 `ScanView`、`FindingsView`、`RulesView`、`HistoryView`、`SettingsView` 位于 `codescan/apps/desktop/src/views/`；`CopyButton` 与 `DirectoryPicker` 留在 `components/`；所有 import 使用新路径。
- **T4 规则职责**：`codescan/crates/engine/rules/` 只含内置规则 YAML；全部规则 fixture 位于 `codescan/crates/engine/tests/fixtures/rules/<fixture>/`；`rule_suite` 从新目录读取并保持原有精确命中判定。
- **T5 工程引用**：Release workflow 的所有 `run` step 统一在 `codescan/` 执行，读取 `target/` 即 `codescan/target/`；`setup-node` 使用 `cache-dependency-path: codescan/pnpm-lock.yaml`，`rust-cache` 使用 `workspaces: codescan`；发布资产目录与 upload-artifact `path` 字面一致为 `codescan/artifacts/release-publish/`。Dependabot 的 Cargo/npm 目录为 `/codescan` 且对应锁文件存在。根 Taskfile 的 stapler 路径为 `codescan/target/`，clean/scan 通过 npm scripts 委托，不再直调旧 `apps/desktop` 或根 `cargo run`。`.gitignore` 覆盖 `codescan/apps/desktop/src-tauri/gen/`、生成的 `tauri.ci.conf.json`、`codescan/target/` 与 `codescan/artifacts/`。`.codescanignore` 位于扫描根 `codescan/` 并保持 `examples/` 忽略；`task scan` 扫描 `codescan/`。根 `AGENTS.md`、PR 模板与贡献指南中的 Cargo 命令显式进入 `codescan/` 或使用 manifest 路径，文档链接指向新位置。
- **T6 官网边界**：`website/README.md` 与 `website/AGENTS.md` 使用 `codescan/assets/`、`codescan/crates/` 和 `codescan/apps/desktop/` 作为事实/资产来源；从 `website/` 执行文档中的品牌与截图复制命令后，目标文件与 `codescan/assets/` 对应源文件逐字节一致。官网仍无包管理清单、构建步骤和外部运行时依赖。
- **T7 行为回归**：在仓库根执行 `task check` 全通过，覆盖 TypeScript 类型检查、Vitest、前端生产构建、Clippy `-D warnings` 和 Rust 全工作区测试；`task test:cli` 单独通过。
- **T8 文档一致**：英文根 README 只承担仓库产品导航并链接 CodeScan 中文 README，根不保留 CHANGELOG 副本；CodeScan 双语 README、`DESIGN.md` 与 `CHANGELOG.md` 位于 `codescan/`。CodeScan 文档内路径以 `codescan/` 为当前根，指向共享规格或治理文件时使用 `../.specs`、`../AGENTS.md` 等相对路径，指向官网时使用 `../website/`；`DESIGN.md` 的页面路径与官网现状同步。本任务 Plan 与验证结论可回指 T1–T7。

## 边界与失败语义

- 这是干净切换，不保留旧目录的转发配置、兼容副本或重复工作区；任一旧路径引用缺失即视为迁移不完整。
- 目录迁移不得修改 Finding、ScanEvent、Stats、规则命中、CLI 输出、IPC 或持久化语义。
- 不删除旧的本地产物目录；新构建产物写入 `codescan/target/` 与 `codescan/artifacts/` 并保持忽略。
- CI 配置采用静态检查与本地等价命令验证；未实际打 tag 时不得声称 Release 已实跑。

## 排除项

- 不拆分 `llm.rs`、`commands.rs`、`metrics.rs` 或污点分析大文件，不改变公开 API。
- 不引入前端 feature-based 状态重组、路由、新框架或新抽象。
- 不调整官网页面内容、样式、脚本、部署方式或 GitHub Pages 设置。
- 不执行真实发布、签名、公证或 DMG 构建。
- 不修订 `.specs/domain/scan-engine.md`：本任务不改变领域规则或长期业务边界。
