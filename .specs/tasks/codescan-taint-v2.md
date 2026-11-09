---
status: done
---

# Task Spec：Codescan v2.1 —— 过程内污点追踪

task-id: `codescan-taint-v2`

## 背景与已定决策

Domain Spec §4a 已定义污点的领域契约：过程内近似、声明式端点、顺序不敏感、发现模型（span = sink，证据携带 source 位置列表）、指纹沿用 v1 规则。LLM 辅助层在污点落地后另行开任务。

语言范围（已定）：**v2.1 先行 TypeScript + Python 两族**；Java/Go/Rust 污点端点声明为 v2.2。过程内、变量名扁平化、无堆别名是既定近似。

## 范围

### 引擎

- 规则 YAML 扩展 `taint:` 块：`sources` / `sinks` / `sanitizers` 各为该语言的 query 列表（均须含 `@match` 捕获），编译期校验与 AST 规则同标准。
- 引擎新增污点分析：函数体遍历（复用度量 Profile 的函数节点表）、赋值传播（每语言赋值/声明节点表）、变量名扁平化的污染集合、sanitizer 命中即清洗、source 命中记录首次位置。
- 发现：span = sink 命中；`flows` 证据 = [source 位置列表]；走统一发现管线（指纹、排序、degraded、SARIF）。
- SARIF `properties.flows` 输出污点链；UI 发现详情展示污点流。

### 内置污点规则（v2.1）

- TypeScript ≥4 条：函数参数 / `prompt()` / `process.argv` / `req.query` 类 source → `eval` / `innerHTML` / `exec(query)` / `child_process` 类 sink；`parseInt`/`Number` 声明为 sanitizer 示例。
- Python ≥4 条：函数参数 / `request.args` / `input()` → `eval` / `exec` / `os.system` / SQL 拼接 sink。
- 每条污点规则带 fixture（正例：可达；负例：sanitizer 切断、source 与 sink 同函数但无赋值链、跨函数不报）。

## 验收标准（全部可客观判定）

- **B1 确定性**：含污点语料的双重扫描字节相同（扩展既有 A1 测试）。
- **B2 污点 fixture**：每条污点规则 fixture 覆盖「可达命中 / sanitizer 切断 / 无链不报 / 跨函数不报」四类用例；套件精确集合相等判定沿用 A2。
- **B3 零回归**：v1 全部验收测试与规则套件仍全绿。
- **B4 证据链**：SARIF `properties.flows` 含 sink→source 位置列表，与内部发现一一对应（扩展 A5 断言）。
- **B5 性能**：A9 语料（1000 文件）扫描仍 ≤ 90s。
- **B6 UI**：发现详情对污点发现渲染 source→sink 链（vitest 断言）。

## 排除项

- 跨函数/过程间分析、堆别名、字段敏感与路径敏感合并（记录为已知近似）；Java/Go/Rust 污点端点（v2.2）；LLM 辅助层（独立任务）。

## 失败语义补充

- `taint` 块 query 编译失败：与 AST 规则同样整体拒绝（Domain §3.3）。
- sink/source 命中但函数体外：不报（过程内边界）。

## 结论（2026-11-09，已实施）

v2.1 落地。引擎 `taint.rs`：端点声明式编译、函数体边界、赋值链传播（各语言赋值节点表）、sanitizer 切断（RHS 重叠与 sink 内标识符范围两层）、直接流与变量链合并、sink 判定 = 种类+字节范围双等值（消除 python block/expression_statement/call 同范围重复）、`flows` 证据贯通发现模型 → SQLite → UI → SARIF `properties.flows`。内置污点规则 8 条（TS 3、Py 5），fixtures 覆盖可达/sanitizer 切断/无链/跨函数四类。

实现中确认并记录的两个 tree-sitter 事实：①同名 `@match` 捕获的顺序不保证，端点取「跨度最小」节点；②sink 节点种类必须参与相等判定。

验证：clippy 0 警告；`cargo test --workspace` 全绿（引擎单测 12，其中污点 5；集成 8，含 B1 确定性语料含污点、B4 SARIF flows；规则套件全量含 8 条污点规则与堆叠标注）；`tsc`、vitest 2（含 B6 污点链渲染）、`vite build` 全绿。

附带修订：fixture 标注语义升级为「连续标注指向其后第一个非空且非标注行」，支持同一行多规则命中（v1 Task Spec 的 A2 一节已同步）。

遗留：②v2.2 = Java/Go/Rust 污点端点；跨函数/堆别名/字段敏感仍为已知近似（Domain §4a）；③LLM 辅助层待开任务。
