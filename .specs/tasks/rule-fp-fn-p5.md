---
status: done
---

# Task Spec：Go/Rust taint 导入别名

task-id: `rule-fp-fn-p5`

## 背景

Pattern 规则已支持 Go/Rust 导入别名；taint 规则仍漏掉包别名/点导入/通配导入：

- Go：`import ex "os/exec"` / `import . "os/exec"` 等。
- Rust：`use std::process::Command as Cmd`、`use std::fs as f`、`use std::fs::*` 等。

## 范围

- taint 规则支持 `imports` 文件级正则 gate；`imported_symbols` 仍仅用于 pattern。
- 新增 Go 别名/点导入 taint 规则：
  - `go/taint-exec-param-alias`
  - `go/taint-open-param-alias`
  - `go/plaintext-http-alias`
- 新增 Rust 别名/通配导入 taint 规则：
  - `rust/taint-command-param-alias`
  - `rust/taint-fs-write-param-alias`
- gate 精确到别名/点/通配导入，普通导入仍由原规则负责，避免重复发现。

## 验收

- [x] Go 别名与点导入的 exec/open/http sink 命中。
- [x] Rust `use ... as` 与通配导入的 Command/fs write sink 命中；嵌套通配导入命中。
- [x] 普通导入不触发新别名规则（避免重复）。
- [x] 规则套件全量通过；规则数量统计更新为 110 / Go 16 / Rust 19 / security 94。
- [x] `task check` 全绿。

## 排除项

- Go `_` 空白导入（无符号）。
- taint source/sanitizer 的别名解析；当前只 gate sink 文件。
- Rust 其他 std 模块别名的 taint。
