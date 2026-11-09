---
status: done
---

# Task Spec：Codescan v4.1 —— 同文件过程间污点

task-id: `codescan-taint-v4`

## 背景与边界

Domain §4a 的污点语义扩展为**同文件跨函数**近似：按调用名索引本文件函数（重名取并集），位置参数映射形参，返回值可携带污点；布尔摘要不动点迭代（支持递归）。跨文件、堆别名、字段敏感仍为已声明近似，不得声称。
既有 v2 全部语义与 17 条污点 fixtures 必须零回归。

## 范围

- 函数索引与形参提取（TS/TSX/JS/JSX/Python/Java/Go/Rust，复用度量 Profile 的函数节点表）。
- 调用点解析：`f(...)` / `obj.f(...)` / `obj.f` 方法调用（按末段名字）；位置参数（跳过关键字参数）；无法解析的调用保守忽略。
- 摘要不动点：每函数每形参 → {可到达 sink, 返回值可被污染}；迭代顺序按函数起点确定性排序，单调收敛。
- 最终收集：从函数内 source（含作为 source 的形参）出发，沿调用链递归收集 sink 发现（深度上限 6、环检测），发现 span = sink 节点，flows = 源头位置集合；同一 (规则, 路径, sink 范围) 合并去重并合并 flows。
- 内生 source 语义保持 v2：每个函数形参仍是 source；跨函数调用只增加来自调用者实参的流。

## 验收标准（全部可客观判定）

- **D1 调用链 sink**：同文件 `handle(param) → run(param) → eval(param)` 命中 sink，flows 含 handle 参数位置。
- **D2 返回值流**：`v = helper(tainted); eval(v)` 命中；helper 返回常量时不命中。
- **D3 sanitizer**：调用边界与函数体内两处切断均生效（各一负例）。
- **D4 递归与确定性**：互递归函数不挂起、双重扫描发现 JSON 字节相同。
- **D5 跨文件不报**：双文件语料中跨文件流不产生发现（负例断言）。
- **D6 重名并集**：同文件两个同名函数之一可达 sink 时按并集命中（fixture 断言）。
- **D7 零回归**：全量规则套件（含 17 条污点 fixtures）与既有集成/单测全绿。
- **D8 性能**：A9 语料（1000 文件）仍 ≤ 90s。

## 排除项

跨文件/跨模块、堆别名、字段敏感、路径敏感合并、动态调用（字符串反射）。

## 失败语义

- 摘要迭代超过函数数×形参数上限仍不收敛：视为实现缺陷（测试断言收敛）；运行时按已得摘要输出并保证确定性。
- 调用名歧义（同名多函数）：并集过近似，写入规则文档。

## 结论（2026-11-09，已实施）

v4.1 落地于 `taint.rs` 重写：函数索引（形参按语言提取，TS `required_parameter`/Py `typed/default`/Java `formal_parameter`/Go `parameter_declaration`/Rust `parameter.pattern`，无名参数占位）、调用点解析（call/method_invocation，取 `member/field/attribute/scoped` 末段名，跳过关键字参数）、每形参 sink 可达摘要 + 返回值内部污点摘要的不动点迭代（互递归验证）、最终收集沿调用链递归（深度 6、环剪枝）并按 sink span 合并 flows。

验收：D1 调用链含调用者 flows（单测）；D2 返回值流（`return input()` → 调用方 `eval(v)`，单测）；D3 调用边界与体内 sanitizer 均切断（单测）；D4 互递归终止且双跑一致（单测）；D5 跨文件不追踪（集成负例，自定义 readInput/eval 规则 0 命中）；D6 重名并集（单测）；D7 全量规则套件（新增仅 v4 可命中的 `py-taint-eval-input/cross_function.py`）与既有测试全绿；D8 A9 语料集成测试仍通过。Domain §4a 与 docs/rules.md 同步更新。
