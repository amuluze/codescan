---
status: done
---

# Plan：CodeScan 与官网分根及 CodeScan 目录分层整理

对应 Task Spec：`.specs/tasks/codescan-repository-layout.md`；领域约束：`.specs/domain/scan-engine.md`。本文件只描述迁移路径，不定义额外验收。

## 目标布局

```text
<repository>/
├── codescan/                    # CodeScan 完整 Rust/pnpm 工作区
│   ├── apps/desktop/
│   │   └── src/
│   │       ├── components/      # CopyButton、DirectoryPicker
│   │       └── views/           # 五个页面组件
│   ├── crates/engine/
│   │   ├── rules/               # 仅内置 YAML
│   │   └── tests/fixtures/rules/# 规则夹具
│   ├── assets/ docs/ examples/ scripts/
│   ├── .codescanignore
│   ├── Cargo.toml / Cargo.lock / rust-toolchain.toml
│   ├── package.json / pnpm-workspace.yaml / pnpm-lock.yaml
│   └── README.md / README.zh-CN.md / DESIGN.md / CHANGELOG.md
├── website/                     # 独立静态官网，无构建步骤
├── .specs/ .plans/ .github/     # 跨产品治理与自动化
├── package.json                 # 仅转发到 codescan/package.json
└── Taskfile.yml                 # 仅调用根 npm scripts
```

## Throughput checkpoint

- 阻塞步骤：顶层目录移动必须先完成，后续路径修改都依赖它。
- 共享可变状态：Git 工作树、根/Task 与 CodeScan 两份 package 配置、同一批文档。
- 分解结论：由单个实施者顺序完成，不 fan-out；并行编辑移动源与引用目标会产生丢失更新。
- 验证策略：迁移类改动复用现有 Vitest/Rust/CLI 测试，不新增测试源码；每个配置子步骤用命令解析、构建或静态路径检查证明。

## 步骤与验收映射

| # | 实施步骤 | 主要文件 | 验证与验收 |
|---|---|---|---|
| 1 | 整体移动 CodeScan 工作区、产品文档、资产与锁文件；保留根治理文件、`.specs`、`.plans`、`website`；不移动或删除旧 `node_modules/`、`target/`、`artifacts/` | `codescan/`、根 README | `test -f codescan/{Cargo.toml,Cargo.lock,pnpm-workspace.yaml,pnpm-lock.yaml,rust-toolchain.toml}`；根锁文件不存在；T1、T8 |
| 2 | 新建根编排 package；CodeScan package 增加 clean/scan/scan:sarif 实现；Taskfile 全部委托根 scripts，stapler 使用 `codescan/target/`；更新 `.gitignore` | `package.json`、`codescan/package.json`、`Taskfile.yml`、`.gitignore` | `task --list`；`task install`；依赖目录在 `codescan/`；`git check-ignore` 覆盖新产物；T1、T2、T5 |
| 3 | 移动五个视图并修正 App/共享组件 import；移动规则夹具并修改 runner 常量与错误文案 | `codescan/apps/desktop/src/`、`codescan/crates/engine/{rules,tests}/` | `task typecheck`；`cd codescan && cargo test -p codescan-engine --test rule_suite`；T3、T4 |
| 4 | Release workflow 统一 `defaults.run.working-directory: codescan`，显式 pnpm/rust 缓存工作区，修正 upload 路径；Dependabot 指向 `/codescan` | `.github/workflows/release.yml`、`.github/dependabot.yml` | YAML 可解析；静态核对 working-directory、cache、workspaces、upload path；本地执行 release-config 生成脚本；T5 |
| 5 | 更新根 AGENTS、PR 模板、贡献指南；更新 CodeScan 双语 README/DESIGN/规则文档相对链接；新根 README 只做产品导航 | `AGENTS.md`、`.github/`、`CONTRIBUTING.md`、`codescan/*.md`、`codescan/docs/` | 链接与命令静态检查；规则套件命令以 `cd codescan &&` 开头；T4、T5、T8 |
| 6 | 更新官网事实来源与资产复制命令；执行复制命令并逐文件比较 | `website/README.md`、`website/AGENTS.md` | `cmp` 两个品牌文件与两个截图；无 package/build 新增；T6 |
| 7 | 真实命令与完整回归；记录证据并归档 Spec/Plan | 全仓库 | `task build`；`task dev` 出现 ready 且 1420 监听后终止；`task scan`；`task check`；`task test:cli`；T2、T7 |

## 关键实现决策

- 根依赖安装脚本命名为 `setup`，避免与包管理器 `install` 生命周期钩子重名；其余根 scripts 只做 `pnpm --dir codescan run <script>`。
- `scan` 与 `scan:sarif` 在 `codescan/` 执行，扫描根为 `codescan/`；SARIF 输出为 `codescan/artifacts/codescan.sarif`。`.codescanignore` 同目录保留 `examples/`。
- CI 所有 `run` step 通过 job 级 `defaults.run.working-directory` 进入 `codescan/`，因此既有脚本内的 `apps/desktop`、`target`、`artifacts` 相对路径保持单一含义；只有 action 的仓库根相对路径显式加 `codescan/`。
- 视图与共享组件处于同一目录深度，视图原有 `../stores`、`../platform` 引用不变；只修改 App 页面 import 和视图对共享组件的 import。
- 整体迁移保留 crate 内相对路径；`include_str!`、Tauri 测试的 `CARGO_MANIFEST_DIR` 推算和发布脚本自定位逻辑不改。

## 风险与回滚

- **旧本地产物混淆**：旧根 `target/`、`node_modules/` 不删除；验证只使用 `codescan/` 内新产物。回滚以 Git rename 反向移动，不清理用户本地产物。
- **依赖重复安装**：迁移后执行一次 `task install`，锁文件不变；若 lockfile 仅因环境重解而变化，停止并检查，不提交锁文件噪声。
- **CI 未实跑**：只做 YAML/路径静态验证和本地等价命令；结论明确标记真实 tag Release 未验证。
- **长驻开发服务**：`task dev` 用受管后台服务验证 ready/端口后立即停止，不留后台进程。
- **文档链接漂移**：迁移后只检查受影响引用，不做仓库无关文档改写或大型 Rust 模块拆分。

## 验证结论（2026-11-13）

| 验收 | 已执行证据 | 结果 |
|---|---|---|
| T1 | Node 结构断言检查根/嵌套 package、唯一锁文件、旧根路径消失；`pnpm run setup` 后再次检查根锁文件 | 通过；CodeScan 独立工作区与根纯转发成立 |
| T2 | `task --list`、`task install`、`task build`、`task scan`、`task dev`；`lsof` 检查 1420；浏览器打开扫描页 | 16 个既有任务保留；构建成功；扫描退出 0（files=126、findings=254、events=22）；Vite ready 且页面与五导航渲染，服务已停止 |
| T3 | Node 断言五个 views 与两个 shared components；`task typecheck`；Vitest；生产构建 | 通过；15 个 Vitest 测试通过，37 个模块构建成功 |
| T4 | `cd codescan && cargo test -p codescan-engine --test rule_suite`；Node 断言 rules 仅 6 个 YAML、fixtures 非空 | 规则套件 1 项通过，fixture 新路径生效 |
| T5 | Ruby YAML 字段断言；`git check-ignore` 四项；`node codescan/scripts/generate-release-config.mjs --force-no-updater` | CI cwd/cache/workspaces/upload/Dependabot 全部通过；生成配置 updater=disabled；忽略规则全部命中 |
| T6 | 在 `website/` 执行文档两条 `cp` 命令，并对两个品牌文件与两个截图逐一 `cmp` | 四对文件逐字节一致，官网未引入构建链 |
| T7 | 根 `task check && task test:cli` | 完整门禁通过；CLI 单元 3 项与黑盒 9 项通过 |

补充说明：

- 纯浏览器验证出现既有的 Tauri IPC `transformCallback` 错误；页面仍正常渲染。该模式不提供原生 IPC，本任务未修改此行为。
- 首次安装曾重新生成根 `pnpm-lock.yaml`；删除后分别执行 `pnpm run typecheck`、`pnpm --dir codescan install --frozen-lockfile`、`pnpm run setup`，均未再生成，最终结构断言通过。
- 未执行真实 tag Release、签名、公证或 DMG 构建；CI 结论仅覆盖静态配置与本地等价命令。
- 最终只读审查：approve；T1–T8 全部满足，无阻塞项或额外行为变更。
