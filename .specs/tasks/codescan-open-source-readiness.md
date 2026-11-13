---
status: done
---

# Task Spec：开源发布准备修复

task-id: `codescan-open-source-readiness`

## 背景与目标

终审发现：缺少 push/PR CI，发布包未携带第三方许可声明，v1.0.0 变更日志未归档，部分社区与安全入口不可用。本任务修复仓库内缺口，并配置当前账号权限与私有仓库套餐允许的 GitHub 安全设置；不擅自改变仓库可见性。

## 范围与验收

### 1. 持续集成与分支保护

- 新增 `.github/workflows/ci.yml`，在 `main` push 和 PR 上执行冻结依赖安装、TypeScript 类型检查、Vitest、前端构建、Clippy `-D warnings` 与 Rust 全工作区测试。
- CI 使用 Node 22、pnpm 11、Rust 1.96.0，路径和缓存均指向 `codescan/` 工作区。
- 验收：YAML 可解析；本地 `task check` 通过；推送后真实 GitHub Actions push 运行成功。分支保护仅在 GitHub 当前仓库套餐允许时配置，否则明确遗留到公开后。

### 2. 第三方许可声明

- 提供确定性的声明生成脚本，覆盖实际分发的 Rust 与生产 JavaScript 依赖，保留包名、版本、许可证及依赖包内的 license/notice 文本；缺少必要文本时失败，不静默省略。
- `codescan/apps/desktop/src-tauri/THIRD_PARTY_NOTICES.txt` 是构建产物，由 `.gitignore` 排除，不提交；桌面开发、应用构建和发布构建前自动重新生成，并作为 Tauri bundle resource 打入 `.app`。
- CI 与 `task check` 执行生成器并验证其成功，不再比较已提交文件；Dependabot 或锁文件变化无需额外提交生成物。
- 验收：生成器成功产出声明；仓库不跟踪声明文件；构建后的 `.app` 含 `Contents/Resources/THIRD_PARTY_NOTICES.txt`；发布文档说明生成方式。

### 3. 安全与社区配置

- 在当前权限允许时启用 Dependabot vulnerability alerts/security updates、private vulnerability reporting、secret scanning/push protection。
- 修正 Issue 模板中指向未启用 Discussions 的链接；行为准则使用实际可用的私密维护者联系渠道。
- 验收：通过 GitHub API 读回启用状态；私有仓库或 token 权限不支持的设置明确记录，不伪报成功；文档链接与仓库功能一致。

### 4. 发布记录

- 将 v1.0.0 已包含的变更归入 `1.0.0`（2026-11-12），保留后续未发布变更到 `Unreleased`。
- CI 声明必须指向真实工作流；版本记录与应用 `1.0.0` 一致。
- 验收：中英文发布说明及本地文档链接有效；不再声称不存在的 CI。

## 排除项

- 不公开仓库、不改写 Git 历史、不重新发布或替换 v1.0.0 资产。
- 不新增 Windows/Linux 发布矩阵，不引入新的应用运行时依赖。
- 不强制配置私有仓库套餐不支持的 branch protection/Advanced Security。
- 不处理可选的官网 Pages 部署、提交签名改造或 `.pen` 工具占位文件清理。

## 实施记录（2026-11-13）

- 已新增 CI、第三方声明生成器/漂移检查、Tauri resource 和中英文发布文档；v1.0.0 变更记录已归档。
- 生成器覆盖 356 个实际分发包；真实 `task build:app` 后，`.app/Contents/Resources/THIRD_PARTY_NOTICES.txt` 与生成文件一致。过期声明负测按预期失败，`task check` 全量通过。
- 首次远端 CI 暴露出生成器的两类环境依赖：CRLF 规范化与 pnpm 11.9/11.28 对可选 peer 的 `--prod` 语义差异。生成结果统一 LF；JS 清单改为沿 `pnpm list --prod` 中父包真实 `dependencies` 下钻；CI/Release 固定 pnpm `11.28.11`。
- 远端 CI 运行 [43392175560](https://github.com/amuluze/codescan/actions/runs/43392175560) 在提交 `2efab4d` 上通过全部检查（5m30s）。
- 2026-11-13 策略修订：Dependabot 无法更新受跟踪的生成声明，导致依赖 PR 固定失败。改为构建时生成并忽略声明文件，`check:notices` 只验证生成器；本地开发、`.app` 构建和发布构建均先重新生成。该策略经 PR #21 的本地验证与远端完整门禁后合入。
- 仓库公开后已启用 Private Vulnerability Reporting、secret scanning 和 push protection；`main` 已保护：要求 PR、分支保持最新且 `check` 通过，禁止强推/删除，管理员同样受限。GitHub API 已读回确认。Secret scanning 的非 provider 自定义模式与 validity checks 仍显示 disabled（当前仓库功能可用性限制）。
- Dependabot 启用后发现 `glib 0.18.5` 中危告警（GHSA-wrw7-89jp-8q8g，修复版本 0.20.0）。该 crate 不在受支持 macOS 目标依赖图或发布声明中；告警保持打开，等待兼容上游修复，未静默关闭。
- GitHub Actions CI 已提交并推送；真实 push 运行已通过，Issue 联系入口随同次推送生效。仓库公开后 Issue 安全入口已切回 Private Vulnerability Reporting。
