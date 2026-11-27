---
status: done
---

# Task Spec：Go 点导入 / Rust 通配导入规则

task-id: `rule-fp-fn-p4`

## 范围

- Go 点导入 `import . "pkg"`：
  - `os/exec` → `go/exec-shell-command-dot`
  - `crypto/md5` / `crypto/sha1` → `go/weak-hash-dot`
  - `math/rand` / `math/rand/v2` → `go/insecure-random-dot`
- Rust 通配导入 `use path::*`：
  - `std::fs::*` → `rust/fs-read-unbounded-glob`
  - `std::mem::*` → `rust/mem-transmute-glob` / `rust/mem-forget-glob`
  - `std::process::*` → `rust/process-command-new-glob`
- 新规则使用 `imports` 精确限定点/通配导入声明，避免无导入时裸函数名误报。
- 更新内置规则数量与维度统计。

## 验收

- [x] Go 三个点导入规则命中，普通导入不触发点导入规则。
- [x] Rust 四个通配导入规则命中。
- [x] 新规则有正向 fixture，规则套件全量通过。
- [x] README 中英文规则数量/维度统计更新为 105 / Go 13 / Rust 17 / security 89 / quality 13。
- [x] `task check` 全绿。

## 排除项

- Go `_` 空白导入（不引入符号，无调用可报）。
- Rust 嵌套通配导入（如 `use std::{fs::*}`）的完整路径回填。
- taint 规则的点/通配导入。
