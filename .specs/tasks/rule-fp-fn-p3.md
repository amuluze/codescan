---
status: done
---

# Task Spec：规则导入别名 P3（Go/Rust）

task-id: `rule-fp-fn-p3`

## 背景

P2 的别名映射只覆盖 TS/Python；Go 包别名与 Rust `use ... as` 仍会漏报。

## 范围

- Go 导入别名：
  - `import ex "os/exec"` → `go/exec-shell-command`
  - `import m "crypto/md5"` / `"crypto/sha1"` → `go/weak-hash`
  - `import r "math/rand"` / `"math/rand/v2"` → `go/insecure-random`
  - Go 别名映射到包 basename；`/v2` 等版本段回退到上一段。
- Rust `use ... as`：
  - `use std::process::Command as Cmd;` → `rust/process-command-new`
  - `use std::fs as f;` → `rust/fs-read-unbounded`
  - `use std::mem as m;` → `rust/mem-transmute` / `rust/mem-forget`
  - Rust 别名映射到完整路径；对应规则 `imported_symbols` 使用完整路径。
- 支持 Rust 嵌套 use 树（`use std::{fs as f}`、`use std::process::{Command as Cmd}`），按树形前缀重建完整路径。
- 将路径/包名约束从 query 内联谓词移到 metavariable，使别名映射可参与判定。

## 验收

- [x] Go 三个包别名均命中。
- [x] Rust 四个 use 别名均命中。
- [x] 嵌套 use 树别名命中。
- [x] 原有非别名 fixture 仍通过。
- [x] 无别名时不会因 `imported_symbols` 放宽而产生额外命中（规则套件全量校验）。
- [x] `task check` 全绿。

## 排除项

- Go/Rust 点导入、通配导入的完整路径回填。
- taint 规则中的包别名。
