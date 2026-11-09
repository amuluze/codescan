---
status: done
---

# Task Spec：Codescan v6.1 —— SARIF 远端上传（GitHub Code Scanning）

task-id: `codescan-upload-v61`

## 目标与边界

CLI 增加 `github-upload` 子命令，把本地 SARIF 上传到 GitHub Code Scanning REST API：

```
POST https://api.github.com/repos/{owner}/{repo}/code-scanning/sarifs
{ "commit_sha", "ref", "sarif": base64(gzip(sarif)), "tool_name": "Codescan" }
```

- Token 只从环境变量 `GITHUB_TOKEN` 读取（命令行不接收明文 token）；默认参数回退 `GITHUB_REPOSITORY` / `GITHUB_SHA` / `GITHUB_REF`（Actions 环境）。
- `--dry-run`：不联网，输出 endpoint、payload 字节数、commit/ref 校验结果（供 CI 测试与排障）。
- 响应：打印 `id` 与 `url`；HTTP 非 2xx → 可读错误与响应体（截断）、退出码 2；成功退出码 0；参数/文件校验失败退出码 2。
- GitLab 限制记录在文档：GitLab 无 SARIF Code Scanning 等价 API（代码质量报告为专用 JSON），本命令仅面向 GitHub。

## 验收标准

- **U1 构造**：请求体/端点/鉴权头的纯函数单测（含 gzip+base64 往返一致性、commit SHA/ref 校验）。
- **U2 dry-run**：`--dry-run` 输出 endpoint 与 payload 大小、不发起网络请求（测试断言 stdout 与退出码 0）。
- **U3 错误路径**：缺失 token、非法 repo、SARIF 文件缺失、commit SHA 非法 → 退出码 2 且 stderr 含原因；不包含 token 明文。
- **U4 文档**：docs/cli.md 增补用法与 CI 示例（`scan → github-upload`），说明所需权限（`security-events: write`）与 GitLab 限制。

## 排除项

GitLab 专用格式转换、PR 注解、增量上传去重、桌面 UI 上传入口。

## 结论（2026-11-09，已实施）

CLI 新增 `github-upload`：SARIF 校验（必须 2.1.0）→ gzip+base64 → `POST /repos/{repo}/code-scanning/sarifs`（Bearer `GITHUB_TOKEN`，`Accept: application/vnd.github+json`，`X-GitHub-Api-Version: 2022-11-28`，`tool_name=Codescan`）；repo/commit/ref 支持 CLI 参数与 `GITHUB_REPOSITORY`/`GITHUB_SHA`/`GITHUB_REF` 回退；`--dry-run` 输出 endpoint/commit/ref/payload 字节与 token 状态且不联网；错误信息截断响应体、不含 token。

验收：U1 gzip+base64 往返与 repo/SHA 校验单测；U2 dry-run 二进测试（含 CI 环境变量回退）；U3 缺 token / 非法 repo / 非法 commit / 非 2.1.0 SARIF / 缺文件 → 退出码 2 且 stderr 含原因；U4 docs/cli.md 增补用法、Actions 权限示例与 GitLab 限制。验证：CLI 单测 3 + 集成 9 全绿、clippy 0。真实网络上传未在测试中执行（无凭据），dry-run 保证请求构造正确性。
