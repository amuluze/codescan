---
status: done
---

# Task Spec：跨函数标签传播约束

task-id: `rule-fp-fn-p18`

## 结论

JS/TS/Go/Rust 的 `break label` / `continue label` 目标必须在同一函数内；跨函数“传播标签”不是合法语义。实现应保持函数隔离，禁止内层函数标签或控制流影响外层常量分析。

## 范围

- 嵌套函数不参与外层常量与标签流分析。
- 内层函数的 label/break/continue 不改变外层循环或块状态。
- 添加回归测试固定该隔离语义。
- 文档记录标签为函数局部。

## 验收

- [x] 嵌套函数内 label/break 不影响外层 `flag` 常量。
- [x] 外层标签不会泄漏到内层函数。
- [x] 原有 labeled break/continue 与循环合并测试通过。
- [x] taint 单测与全量门禁通过。

## 排除项

- 不实现跨函数标签传播（语言语义不允许）。
