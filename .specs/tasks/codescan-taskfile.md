---
status: done
---

# Task Spec：Taskfile 任务管理与 macOS Apple Silicon 构建收口

task-id: `codescan-taskfile`

## 目标

参照 axiom 的 Taskfile 约定补齐任务入口：Taskfile 仅做编排，命令实现留在 `package.json` scripts（单一权威来源，CI 与文档引用同一命令）。构建平台收口为 **macOS Apple Silicon (arm64)**；Windows / Linux 暂不支持。

## 范围

- 根 `Taskfile.yml`：install / dev / dev:app / build / build:app / build:dmg / clean:build / typecheck / test / test:rust / test:cli / check:rust / check / scan / scan:sarif。
- `package.json` 增补 scripts：`check`（完整门禁）、`test:cli`、`build:app`、`build:dmg`（`--target aarch64-apple-darwin`）。
- `build:app` / `build:dmg` 加 `platforms: [darwin]` 与 `uname -m == arm64` 前置校验。
- `.github/workflows/release.yml` 收敛为 `macos-14 / aarch64-apple-darwin` 单目标；移除 Linux apt 依赖与 Windows 条目。
- README / AGENTS 更新：平台支持声明、Taskfile 命令表、构建与发布口径。
- `.gitignore` 忽略 `artifacts/`（scan:sarif 产物）。

## 验收标准

- **T1 编排一致**：`task --list` 可枚举全部任务；每个 task 的命令均为对应 npm script（无重复实现）；`task check` 完整通过。
- **T2 平台约束**：`build:app` / `build:dmg` 声明 darwin 平台且 arm64 前置校验生效（非 arm64 时 precondition 失败）。
- **T3 macOS 构建**：`task build:dmg` 在 Apple Silicon 上产出 `Codescan.app` 与 `Codescan_<ver>_aarch64.dmg`，产物路径在 `target/release/bundle/`。
- **T4 发布矩阵**：release workflow 仅含 aarch64-apple-darwin 构建；README 明确 Windows/Linux 暂不支持。
- **T5 文档**：README 命令表以 task 为入口并注明委派关系；AGENTS 验证习惯改为 task 命令。

## 排除项

Windows/Linux 构建与 CI 矩阵、macOS Intel（x86_64）构建、签名与公证凭据。

## 结论（2026-11-09，已实施）

- Taskfile：16 个任务（`task --list` 可枚举），全部委托 `package.json` scripts；`build:app`/`build:dmg` 声明 `platforms: [darwin]` 并前置校验 `uname -m == arm64`；`clean:build`、`scan`、`scan:sarif`（产物 `artifacts/codescan.sarif`，已 gitignore）。
- npm scripts 增补：`check`（typecheck+vitest+build+clippy+cargo test）、`test:cli`、`build:app`、`build:dmg`（`--target aarch64-apple-darwin`）。
- 发布矩阵收敛：release workflow 仅 `macos-14 / aarch64-apple-darwin`，移除 Linux/Windows 条目与 apt 依赖；draft release 说明标注 Windows/Linux 暂不支持。
- 文档：README 平台声明 + Taskfile 命令表（注明委派关系）+ 结构更新（CLI / Taskfile / 五视图）；AGENTS 验证习惯改为 `task check` 等，并明确「先 npm script 再 task」。
- 验证：`task check` 通过（29s）；`task test:cli` 9 项；`task check:rust` 0 警告；`task scan:sarif` 产出 267KB / 234 results（SARIF 2.1.0）；`task build:dmg` 产出 `Codescan.app` 与 `Codescan_0.1.0_aarch64.dmg`（`target/release/bundle/`）。
