---
status: done
---

# Task Spec：taint source/sanitizer 别名

task-id: `rule-fp-fn-p6`

## 背景

Go taint 规则的 sink 已支持别名/点导入，但 sanitizer 仍要求 `strconv.Atoi` / `filepath.Clean` 的原包名，导致别名或点导入时误报。

## 范围

- `strconv.Atoi` sanitizer：
  - 任意 `<pkg>.Atoi(...)`（别名）。
  - 裸 `Atoi(...)`（点导入）。
  - 覆盖 `go/taint-exec-param`、`go/taint-exec-param-alias`、`go/taint-sql-param`。
- `filepath.Clean` sanitizer：
  - 任意 `<pkg>.Clean(...)`。
  - 裸 `Clean(...)`。
  - 覆盖 `go/taint-open-param`、`go/taint-open-param-alias`。
- 新增 `py/taint-os-system-request-alias`：识别 `from flask import request as req` 与 `import flask as f` 的 `.args` source。
- 补充别名/点导入 sanitizer 负例（不应产生 taint 发现）。
- 清理 `go/insecure-random-dot` 的重复 `dimension` 字段。

## 验收

- [x] 别名 `s.Atoi`、点导入 `Atoi` 能切断 taint。
- [x] 别名 `f.Clean`、点导入 `Clean` 能切断 taint。
- [x] Flask request 模块/对象别名 `.args` source 命中。
- [x] 原直接 sanitizer 与无 sanitizer fixture 行为不变。
- [x] 规则套件全量通过。
- [x] `task check` 全绿。

## 排除项

- 包级 source 别名（当前 Go/Rust taint source 均为函数参数或字面量，无包名端点）。
- 跨函数别名解析。
