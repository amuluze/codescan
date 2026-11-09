# codescan

本地优先的代码扫描桌面工具。自研 AST 规则引擎（tree-sitter），三维覆盖安全漏洞、业务逻辑、代码质量；零外部引擎依赖、单机可运行、扫描只读。

## 背景与决策

- 选型调研：[reports/2026-11-06-open-source-code-audit-tools.md](reports/2026-11-06-open-source-code-audit-tools.md)
- 领域不变量（最高约束）：[.specs/domain/scan-engine.md](.specs/domain/scan-engine.md)
- v1 规格与验收：[.specs/tasks/codescan-desktop-v1.md](.specs/tasks/codescan-desktop-v1.md)
- 实施路径：[.plans/codescan-desktop-v1.md](.plans/codescan-desktop-v1.md)

关键取舍（已确认）：复刻 axiom 技术栈（Tauri 2 + React 19 + Rust/SQLite）；引擎自研而非编排外部工具；确定性规则 + 质量度量为基线，污点分析按同文件过程间近似（v4，跨文件/堆敏感明确排除），LLM 仅作分诊/候选规则辅助且永不进门禁。

## 结构

```text
codescan/
├── crates/engine/        # codescan-engine：规则编译、匹配、度量、指纹、SARIF、基线（无头可测）
│   ├── rules/            # 内置规则（按语言分文件）+ fixture（ruleid: 标注）
│   └── tests/            # 集成测试（确定性/fail-closed/SARIF/语言矩阵/性能）
├── apps/desktop/         # Tauri 2 + React 19 桌面应用
│   ├── src/              # 四视图：扫描 / 发现 / 规则 / 历史
│   └── src-tauri/        # 命令层 + SQLite（~/.codescan/codescan.db）
└── docs/rules.md         # 规则编写指南（模式语言、fixture 格式）
```

## 开发

环境要求：Node ≥ 22、pnpm ≥ 10（包管理器）、Rust 1.96（rust-toolchain.toml 锁定）。

```bash
pnpm install
npm run tauri dev        # 桌面应用（开发模式）
```

| 命令 | 作用 |
|---|---|
| `npm run test` | 前端 vitest |
| `npm run typecheck` | TS 类型检查 |
| `npm run build` | 前端生产构建 |
| `npm run test:rust` | Rust 全量测试（引擎 + 命令层无头） |
| `npm run check:rust` | clippy -D warnings |
| `cargo test -p codescan-cli` | CLI 黑盒测试 |
| `pnpm tauri build` | 本地产出桌面安装包（未签名） |

规则编写与 fixture 规范见 [docs/rules.md](docs/rules.md)。

## 命令行与 CI 门禁

```bash
# 生成基线（存量问题入库）
cargo run -p codescan-cli -- scan <repo> --save-baseline codescan-baseline.json

# 门禁：仅新增发现阻断（配合 --format sarif 上传/归档）
cargo run -p codescan-cli -- scan <repo> \
  --baseline codescan-baseline.json --fail-on new \
  --format sarif --out codescan.sarif
```

退出码：0 通过 / 1 门禁命中 / 2 用法或运行时错误。完整用法见 [docs/cli.md](docs/cli.md)。

## 忽略规则

扫描范围 = 工作区 `.gitignore` + `.codescanignore` + 全局 `~/.codescan/ignore`（均为 gitignore 语法）+ 内置默认目录（`node_modules`、`target`、`dist`、`build`、`.venv`、`__pycache__`、`.next`、`coverage`）。被忽略文件计入扫描事件的汇总计数。

## LLM 辅助层（v3）

设置页配置 OpenAI/Anthropic 兼容 Provider（BYO Key，密钥仅存本地 SQLite secrets 表）。发现详情支持「LLM 分诊」（参考，不参与门禁）与「生成候选规则」（经编译校验、显式保存后写入自定义规则并影响后续扫描）。详见 [docs/llm.md](docs/llm.md)。

## 门禁语义

任意历史扫描可设为基线；门禁差异 = 新扫描发现 \ 基线（仅指纹相等性）。指纹对行号漂移稳定，实体代码变更即视为新发现。SARIF 2.1.0 导出与内部发现一一对应。详见 Domain Spec。

## 发布

打 tag（`v*`）触发 `.github/workflows/release.yml`：macOS arm64 / Linux x64 / Windows x64 未签名构建，产出 draft release。签名与公证需在仓库 Secrets 注入证书后在 workflow 中启用（当前未配置，macOS 首次打开需右键 → 打开）。
