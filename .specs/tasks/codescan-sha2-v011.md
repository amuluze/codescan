---
status: done
---

# Task Spec：sha2 0.11 指纹编码迁移

task-id: `codescan-sha2-v011`

## 背景与目标

Dependabot PR #18 将引擎 `sha2` 从 0.10 升到 0.11。`digest 0.11` 的 `finalize()` 返回 `hybrid-array::Array`，不再实现 `LowerHex`，导致指纹与内容哈希两处 `format!("{:x}", ...)` 编译失败。升级必须保持既有 SHA-256 摘要与指纹字符串完全不变。

## 范围与验收

- `codescan-engine` 使用 `sha2 = "0.11"`；Tauri 等其他依赖仍可使用 `sha2 0.10`，允许锁文件并存。
- 用显式小写十六进制编码替换两处 `LowerHex`：
  - `content_hash` 规范化文本哈希；
  - Finding 指纹最终摘要。
- 新增 known-answer 测试：
  - `sha256("abc")` 的既有标准值；
  - 固定 Finding 的完整指纹保持 `eae0a06753553f1fd5a3f71d29e290785da9c6bd3688774553e6d396b08e2fca`。
- 验收：`task check` 通过；既有指纹稳定性、基线、规则与 CLI 测试不回归；第三方声明生成成功；PR CI 通过。

## 排除项

- 不改变指纹算法、输入分隔符、规范化规则或排序。
- 不升级 Tauri 及其 `sha2 0.10` 依赖。

## 验证（2026-11-13）

- `cargo test -p codescan-engine --lib fingerprint::tests`：7 项通过，含新增内容哈希与完整指纹 known-answer 测试。
- 根 `task check`：通过；生成 362 个第三方包声明，TypeScript、Vitest、Clippy、Rust 全工作区测试均通过。
- 锁文件同时保留 `sha2 0.11.1`（引擎）与 `sha2 0.10.9`（Tauri）以及 `minisign-verify 0.3.0`。
- 由替代 PR 交付并关闭 Dependabot PR #18；最终以远端 CI 为准。
