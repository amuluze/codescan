---
status: done
---

# Task Spec：运维与可用性收口（噪声调优 / 忽略源 / 打包 / CLI）

task-id: `codescan-ops-v4`

## 背景与目标

四项剩余缺口：①度量在真实仓库信噪比差（axiom 首扫 1591 条，function-length 507 / duplicate 424 / cyclomatic 357）；②忽略源仅 `.gitignore`；③无打包链路与发布产物；④无 CLI，CI 无法做门禁（引擎只有库与桌面入口）。

## 范围与验收

### 1. 度量噪声（已完成设计并实施）

- 语义修正：外层函数的环境复杂度/嵌套不再计入嵌套函数体（其度量归属嵌套函数自身）。
- 阈值依据真实仓库分布（axiom 违规集）：cyclomatic 15→25、function-length 80→200、duplicate window 10→12；nesting 保持 5（仅 3 条，非噪声源）。
- 验收：新增嵌套排除单测（外层 2 / 内层 4 精确断言）；阈值调整后 5 个度量 fixture 更新并全绿；重扫 axiom 度量发现显著下降（记录数值）。

### 2. 忽略源可配置

- 扫描范围 = `.gitignore` + `.codescanignore` + `~/.codescan/ignore`（全局，gitignore 语法）+ 内置默认目录（node_modules/target/dist/build/.venv/__pycache__/.next/coverage）。
- `ScanOptions::global_ignore_file` 由桌面层注入；被忽略文件计入 `IgnoredSummary`。
- 验收：集成测试构造四类忽略源，断言仅真实源码被扫描且汇总事件计数正确；去掉全局忽略选项后可扫描到对应文件（选项生效性反证）。

### 3. 打包链路

- `bundle.active=true`，平台图标集（icns/ico/png）由 `tauri icon` 生成并入库；release workflow（tag `v*`，macOS arm64 / Linux x64 / Windows x64，tauri-action，未签名 draft release）。
- 验收：本地 `tauri build` 产出 macOS 安装包（记录产物路径/大小）；workflow YAML 结构审查通过；签名/公证作为 secret 注入点注释记录（未签名产物明确标注）。

### 4. CLI + 门禁退出码

- 新 crate `crates/cli`（bin `codescan`）：`scan <root>` 支持 `--format text|json|sarif`、`--out`、`--rules`、`--global-ignore`、`--baseline`、`--save-baseline`、`--fail-on new|error|warning|none`；退出码 0/1/2。
- 验收：6 项黑盒集成测试（干净退出、格式输出、基线门禁新增阻断、SARIF、非法规则、全局忽略）；clippy 0。

## 排除项

代码签名/公证凭据、macOS Intel 构建、自动更新、Homebrew/AUR 分发、远端 SARIF 上传。

## 失败语义

- 忽略模式非法：best-effort 忽略该模式，不阻断扫描。
- 基线文件缺失/损坏：CLI exit 2，stderr 给出原因。
- 打包无签名凭据：产物标注未签名，不视为失败。

## 结论（2026-11-09，已实施）

1. **度量噪声**：语义修正（嵌套函数体不再计入外层复杂度/嵌套）+ 阈值依据 axiom 实测分布调整（cyclomatic 15→25、function-length 80→200、duplicate window 10→12，nesting 保持 5）。实测：axiom 全量发现 1591 → 1493（语义修正）→ **735**（新阈值）；度量分项 cyclomatic 357→99、function-length 507→115、duplicate 424→222、nesting 7→3。新增嵌套排除单测（外层 2 / 内层 4）；度量 fixtures 随阈值更新。
2. **忽略源**：`.gitignore` + `.codescanignore` + 全局 `~/.codescan/ignore` + 内置默认目录；`ScanOptions.global_ignore_file` 由桌面层注入 `~/.codescan/ignore`；集成测试构造四类忽略源并反证选项生效。
3. **打包链路**：`tauri icon` 生成全平台图标并入仓；`bundle.active=true`（DeveloperTool）；本地 `pnpm --filter @codescan/desktop tauri build` 成功产出 `Codescan.app`（20 MB）与 `Codescan_0.1.0_aarch64.dmg`（6.9 MB）；`.github/workflows/release.yml`（tag `v*`，macOS arm64 / Linux x64 / Windows x64，tauri-action draft release，未签名并注明）；action 输入名已对照 `tauri-apps/tauri-action@v0/action.yml` 核实。
4. **CLI**：`crates/cli`（bin `codescan`）落地；`--format/--out/--rules/--global-ignore/--baseline/--save-baseline/--fail-on`，退出码 0/1/2；6 项黑盒集成测试 + `docs/cli.md`；`cargo test --workspace` 与 clippy 全绿。

遗留：代码签名/公证凭据、macOS Intel 构建、远端 SARIF 上传未做（范围外）。
