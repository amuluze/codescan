# Plan：codescan-desktop-v1

对应 Task Spec：`.specs/tasks/codescan-desktop-v1.md`；领域约束：`.specs/domain/scan-engine.md`。本文件只描述实现路径，不定义验收。

## 布局

```text
codescan/
├── Cargo.toml                  # workspace: crates/engine + apps/desktop/src-tauri
├── package.json                # npm workspace: apps/desktop
├── crates/engine/              # codescan-engine：纯 Rust 库，无 Tauri 依赖，全部引擎验收在此无头测试
│   ├── src/{lib,language,rules,compile,scan,findings,fingerprint,metrics,sarif,baseline,events}.rs
│   ├── rules/                  # 内置规则 YAML（随引擎版本化）
│   ├── rules/tests/            # 规则 fixture（ruleid:/ok: 行标注，semgrep 语义）
│   └── tests/                  # A1/A3/A4/A5/A6/A7/A9 集成测试
└── apps/desktop/               # Tauri 2 + React 19 + Vite + Zustand
    ├── src/                    # UI（Scan/Findings/Rules/History 四视图）
    └── src-tauri/              # 命令层（薄胶水）+ SQLite（rusqlite，Rust 独占）
```

命令层实现为纯函数 + Tauri 薄包装，SQLite 逻辑可脱离 Tauri 无头测试（A8 Rust 部分）。

## 关键设计决策

- **规则模式语言**：直接采用 tree-sitter query（`#eq?`/`#match?` 谓词）为 AST 模式；过滤器 `not`（同语言 query，命中即排除）、`inside`（命中须嵌套于其内）；元变量约束=对捕获再施加正则。文本规则=带语言通配的正则 + 行级命中。
- **发现模型**：`{rule_id, dimension, severity, path, start/end(line,col), message, match_text, captures, degraded, fingerprint}`；排序键 (path, start, rule_id)；指纹 = sha256(rule_id ‖ path ‖ 规范化 match_text ‖ 同内容出现序号)，不含行号（行漂移稳定，Task A4）。
- **度量即内置规则**：复杂度/长函数/深嵌套/重复/不可达以固定 id（`metric/*`）走同一发现管线，quality 维度；阈值在规则 YAML 声明。
- **规则套件**（A2）：`rules/tests/<rule-id>/*.{ts,py,...}`，行上方 `// ruleid: <id>`（按语言注释语法）标注期望命中行，`ok:` 文件/行声明期望不命中；`tests/rule_suite.rs` 遍历执行并断言行集合精确相等。
- **进度与取消**：scan 接收闭包 `on_progress(stage, done, total)` 与 `CancellationToken`；取消即丢弃部分结果，不落库。
- **SQLite**（app 侧）：`scans` / `findings` / `rule_states` / `baselines` / `baseline_entries`；数据根 `~/.codescan/`（0700）。

## 步骤与验证映射

| # | 步骤 | 产出 | 验证 |
|---|---|---|---|
| 1 | 脚手架：Cargo workspace + Tauri 2 + React/Vite/Zustand 可构建 | 空应用启动 | `cargo build`、`npm run build`、`npm run tauri dev`（A10 前置） |
| 2 | 引擎核心：语言注册、walker（gitignore）、parse、规则编译、匹配执行、发现/事件/指纹/排序 | engine crate + 单测 | cargo test 单测 |
| 3 | 度量五项 | 上述同管线 | A6 fixture 单测 |
| 4 | 内置规则 6 语法族 + 通用文本 + 2 业务示例 + 规则套件 harness | rules/ + tests/rule_suite.rs | A2、A7 |
| 5 | SARIF 导出 + 基线 | sarif.rs / baseline.rs | A5、A4 |
| 6 | 确定性/fail-closed/性能测试 | tests/ | A1、A3、A9 |
| 7 | 命令层 + SQLite + 持久化/基线/导出命令 | src-tauri | A8 Rust 侧无头测试 |
| 8 | UI 四视图 + store + 组件测试 | src/ | A8 vitest；A10 人工启动记录 |

步骤 4 的各语言规则与 fixture 在步骤 2 契约冻结后可并行；步骤 8 在步骤 7 命令契约冻结后可并行。

## 风险与回滚

- tree-sitter query 谓词在不同 grammar 上的节点命名差异 → 每条规则必须过 A2 fixture，未过不合并；规则不足额时宁缺毋滥并降低该语言声明数量（回滚到「该语言仅文本规则」需回到 Task Spec 修订，不允许静默）。
- 重复代码度量误报面大 → v1 阈值保守（≥10 规范行），消息标明为启发式。
- Tauri 2 + React 19 组合与本机 Node 22 已验证可用（axiom 同栈）。
