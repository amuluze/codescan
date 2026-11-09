---
status: done
---

# Task Spec：Codescan v5.2 —— Java/Rust 跨文件污点

task-id: `codescan-taint-v52`

## 目标与边界

补齐跨文件解析的两种类型系统边界（Domain §4b）：
- **Java**：`import a.b.C;` 类导入（静态方法调用 `C.run(x)` 经限定名解析）与 `import static a.b.C.run;`；类→文件映射按 `package` 声明 + 文件名（源代码根无关）。
- **Rust**：`use crate::a::b::run;`、`use super::x::y;`、`use self::…`、`as` 别名、`use crate::a::{b, c}`；带 `crate::`/`super::`/`self::` 前缀的限定调用；模块→文件按 crate `src/` 根映射（`a/b.rs` 或 `a/b/mod.rs`）。
- 排除：Java 实例方法/动态派发、TS 命名空间导入、Rust 外部 crate、宏生成的调用。

## 验收标准

- **H1 Java 跨文件（类导入）**：`Utils.run(v)` → `Utils.run` 内 sink，flows 含调用者参数位；无 import 时 0 跨文件（负例）。
- **H2 Java 静态导入**：`import static a.Utils.run;` 后裸调用 `run(v)` → 命中。
- **H3 Rust `use` 导入**：`use crate::util::run;` + `run(v)` → 命中；`use crate::util::run as r;` 别名同断言。
- **H4 Rust 限定调用**：`crate::util::run(v)` 直接调用 → 命中。
- **H5 零回归**：全量污点单测（37）、规则套件、集成、CLI 全绿；Java/Rust 既有 fixtures 不受影响（无跨文件时行为等同 v5.1）。
- **H6 确定性**：双跑一致；A9 性能门限保持。

## 失败语义

- 无法解析的 import/use：该名字不进入跨文件解析表（保守），不报错。
- crate `src/` 根无法判定：Rust 限定调用不跨文件。

## 结论（2026-11-09，已实施）

- 事实层：`CallFact.qualifier`（Java 类/实例接收者、Rust `crate::…` 路径、成员调用裸标识符）；Java `package`+文件名 → class_id，`import a.b.C` 与 `import static a.b.C.run` 解析；Rust `use` 叶子收集（scoped_identifier / use_as_clause 别名 / scoped_use_list 列表 / self 跳过），按 crate `src/` 根映射模块候选（`a/b.rs` 与 `a/b/mod.rs`，根模块候选 `src/lib|main`）。
- 分析层：Java class_id 附加索引 + 每文件「Class.method」限定键；`resolve_call` 限定者优先（Java 键、Rust 路径候选），否则裸名；无导入同名不跨文件。
- 验收：H1 类导入限定调用（flows 含调用者参数位）与无导入负例；H2 静态导入裸调用；H3 Rust `use … as` 别名；H4 `crate::util::run(v)` 限定调用；H5 全量回归绿（引擎 42 单测 / 10 集成 / 规则套件 / CLI 6 / 应用层 21 / 前端 7，clippy 0）；H6 双跑一致、axiom 1015 文件 35s（debug）。
- 修复：module_id 的 match 分支遗漏 Java/Rust 扩展名剥离（曾使 Rust 跨文件解析全部失效）；负例测试行号与自身参数行冲突。
- 边界（Domain §4b）：Java 实例方法与动态派发、TS 命名空间导入、Rust 外部 crate 不解析。
