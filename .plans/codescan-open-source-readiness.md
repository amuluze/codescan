---
status: done
---

# Plan：开源发布准备修复

对应 Task Spec：`.specs/tasks/codescan-open-source-readiness.md`。本文件只描述实施路径，不新增验收。

## Throughput checkpoint

- 阻塞步骤：先确认 Cargo/pnpm 许可元数据能否提供完整文本，再确定生成器实现。
- 可并行工作：CI、文档修正、GitHub 设置互不依赖。
- 共享可变状态：根/CodeScan package 脚本、Taskfile、Tauri 配置和发布文档由单一实施者顺序修改；最后统一运行门禁。
- 分解结论：不 fan-out 编辑，避免共享文件冲突；计划审查后实施。

## 步骤与验收映射

| # | 实施步骤 | 主要文件 | 验证 |
|---|---|---|---|
| 1 | 新增 macOS CI：Node 22、pnpm 11、Rust 1.96.0；job 工作目录为 `codescan/`，pnpm cache 指向 `codescan/pnpm-lock.yaml`，Rust cache workspace 为 `codescan`；依次运行 frozen install、typecheck、Vitest、前端构建、Clippy `-D warnings`、Rust workspace tests 和 notices 生成检查 | `.github/workflows/ci.yml`、`codescan/package.json` | YAML 解析、脚本 dry-run、真实 Actions 运行 |
| 2 | 实现确定性 notices 生成；声明作为 gitignored 构建产物，不提交；加入 Tauri resources、开发/构建/发布链 | `codescan/scripts/generate-third-party-notices.mjs`、`.gitignore`、`tauri.conf.json`、package/Taskfile | 重复生成一致、仓库不跟踪、`.app` 实际包含声明 |
| 3 | 归档 1.0.0 变更记录；更新中英文发布文档与命令 | `codescan/CHANGELOG.md`、`codescan/docs{,/en}/release.md`、README（如需） | 本地链接和版本/CI 声明检查 |
| 4 | 启用 Discussions 并保留已恢复的问答入口；行为准则改用维护者公开邮箱 `314901758@qq.com`，与安全漏洞 Advisory 通道分离，不依赖私密报告开关 | `.github/ISSUE_TEMPLATE/config.yml`、`CODE_OF_CONDUCT.md` | GitHub API 读回 Discussions；YAML/链接检查 |
| 5 | 通过 GitHub API 启用并读回可用安全设置；记录套餐/权限限制 | GitHub 仓库设置 | API 状态证据 |
| 6 | 运行生成器针对性验证、应用打包验证和 `task check`；归档 Spec/Plan | 全改动面 | 所有适用验收通过或明确 INCONCLUSIVE |

## 关键实现决策

- 包名/版本/许可表达式以 `cargo metadata --locked --filter-platform aarch64-apple-darwin` 和 `pnpm list --prod --depth Infinity` 为事实来源；Cargo 从 workspace members 沿过滤后的 `resolve` 依赖图求可达集合，不直接使用未过滤的 `packages` 数组；pnpm 只沿父包真实 `dependencies` 下钻，排除 dev 与可选 peer。按包名/版本稳定排序；不把本机绝对路径写入产物，生成过程不访问网络 API。
- 优先保留各依赖包内原始 license/notice/copying 文本，不去重、不省略各包归属。crate 未随包提供文本时，使用生成器内固定的标准 SPDX 全文并列出 authors/repository；`OR` 表达式选择其中的 MIT 并在文中注明，`AND` 表达式必须包含全部文本。当前已知回退只需 MIT、BSD-3-Clause、MPL-2.0；无作者时使用「the `<package>` authors」并保留 upstream repository。无 authors 且无 upstream repository 时失败退出，并在错误信息中列出包名与许可表达式。未知表达式或所需标准全文缺失时失败。
- 每个包条目统一包含名称、版本、许可表达式、authors/repository（存在时）及正文来源标记（package file 或 standard SPDX fallback）。
- 声明文件是构建产物：由 `.gitignore` 排除，不提交；检查、开发、应用构建和发布构建均重新生成，CI 只验证生成成功，避免 Dependabot 无法更新生成物的固定失败。
- CI 不依赖维护者本机工具，使用仓库锁定的 Node/Rust 版本与 frozen lockfile。
- GitHub 设置只做显式范围中的安全/社区配置，不改变可见性或发布资产。

## 风险与回滚

- **依赖文本不完整**：已知无随包文本的 crate 走固定 SPDX 全文回退；未知许可证、无归属或所需全文缺失时 fail-closed，不联网猜测版权。
- **生成物过大**：保留每个包的完整文本与归属，不做可能丢失归属的去重；不接受只列 SPDX 名称的降级。
- **CI 实跑需推送**：本地验证后如尚未推送，真实 Actions 验收标为待远端执行，不声称已通过。
- **公开后控制项**：仓库转为 public 后已补齐私密报告、secret scanning/push protection 与分支保护；非 provider 自定义模式和 validity checks 仍受 GitHub 功能可用性限制。
- **生成物缺失**：`THIRD_PARTY_NOTICES.txt` 不提交，依赖生成器在每次 check/build/dev 中成功运行；生成失败即阻断，避免发布缺少法律声明。

## 验证结论（2026-11-13）

| 验收 | 已执行证据 | 结果 |
|---|---|---|
| CI 配置 | Python YAML 解析 `.github/workflows/ci.yml`；JSON 解析 package/Tauri 配置；push 运行 | 通过；run 43392175560 全绿 |
| 环境确定性 | CI 负测发现 CRLF 与 pnpm 11.9/11.28 可选 peer 差异；改为 LF 归一、真实 dependencies 下钻、pnpm 固定 11.28.11 | 通过；同一声明在本地与 CI 均生成 356 包 |
| notices 生成 | 删除文件后运行 `pnpm run check:notices`；`git ls-files` 与 `git check-ignore` | 356 包，3,120,106 字节；未跟踪且正确忽略 |
| 生成物策略 | `task check` 从缺失状态重新生成声明 | 通过；无漂移比较，生成失败仍会阻断 |
| 应用打包 | `task build:app`；`cmp` 源声明与 `.app/Contents/Resources/THIRD_PARTY_NOTICES.txt` | 通过；实际资源 3,120,106 字节 |
| 开发入口 | `pnpm --dir codescan run dev:app` 启动时先生成声明并进入 Vite/Tauri | 通过；验证后停止本地开发进程 |
| 完整门禁 | 根 `task check` | 通过，含 15 项 Vitest、Clippy、Rust 全工作区测试与 notices 检查 |
| 文档 | 17 份 Markdown 本地链接检查；YAML/JSON 解析；`git diff --check` | 通过 |
| GitHub 设置 | 仓库公开后 API 启用并读回 Private Vulnerability Reporting、secret scanning、push protection、Dependabot alerts/security fixes；`main` 分支保护启用 | 通过；非 provider 自定义模式与 validity checks 当前不可用，记录为限制 |

本地验证已通过，策略由 PR #21 合入。剩余 GitHub 功能可用性限制与 `glib` 上游告警记录在 Task Spec。
