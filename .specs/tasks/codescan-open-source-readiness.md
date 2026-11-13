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
- 生成 `codescan/apps/desktop/src-tauri/THIRD_PARTY_NOTICES.txt`，作为 Tauri bundle resource 打入 `.app`；发布构建前重新生成。
- 提供漂移检查，依赖或锁文件变化后声明未更新时检查失败。
- 验收：生成器重复运行字节一致；`--check` 能检测过期声明；构建后的 `.app` 含 `Contents/Resources/THIRD_PARTY_NOTICES.txt`；发布文档说明更新方式。

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
- GitHub API 已启用 Discussions、Dependabot vulnerability alerts 和 automated security fixes；行为准则、安全策略与 Issue 安全联系均使用维护者公开邮箱作为可用私密渠道。
- 私有仓库下 private vulnerability reporting 返回 404、secret scanning 返回 422、branch protection 返回 403；这些项目保留到仓库公开后配置，未改变可见性。
- Dependabot 启用后发现 `glib 0.18.5` 中危告警（GHSA-wrw7-89jp-8q8g，修复版本 0.20.0）。该 crate 不在受支持 macOS 目标依赖图或发布声明中；告警保持打开，等待兼容上游修复，未静默关闭。
- GitHub Actions CI 已提交并推送；真实 push 运行已通过，Issue 联系入口随同次推送生效。
