---
status: done
---

# Task Spec：规则审查 P0/P1/P2 收口

task-id: `rule-fp-fn-p22`

## 范围

- P0 taint：
  - 调用点使用 guarded calls，跳过 `if(false)` 等不可达调用。
  - 循环体字段写效果记录循环条件 guard；`while(cond)` 在实参 false 时不回放。
  - switch 内效果暂不建模，避免 case 不匹配误报。
- P1：
  - `ts/child-process-string` 增加 `no_shadow`，本地 `exec`/`execSync` 定义不再被 bare 规则误报。
  - `java/read-object` 增加接收者名称约束，降低类字段场景下自定义 `other.readObject()` 误报。
- P2：
  - `py/taint-os-system-request-alias` 支持 `import flask as f` 的 `f.request.args` 模块属性 source。
- 补对应 fixture/单测。

## 验收

- [x] 循环 false guard 不产生字段污点；true guard 产生。
- [x] `if(false)` 内调用不应用效果；switch case 不匹配不误报。
- [x] 本地 exec 不再被 child_process bare 规则命中。
- [x] Flask 模块别名 source 命中；普通 config.args 不命中。
- [x] Java 字段类中 `other.readObject()` 不命中，`input.readObject()` 命中。
- [x] 全量门禁通过。

## 排除项

- switch case 的精确 guard 回放（当前保守跳过 switch 内效果）。
- 任意字段名/继承字段的 Java 类型推断。
- 任意 Flask 别名链与动态属性访问。
