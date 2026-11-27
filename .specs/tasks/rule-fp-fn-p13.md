---
status: done
---

# Task Spec：条件驱动的 if 分支裁剪

task-id: `rule-fp-fn-p13`

## 背景

if/else 分支合并对所有 if 保守合并，即使条件为常量：

```ts
let flag = true;
if (true) { flag = false; }
return flag ? a : b; // 只应关联 b
```

## 范围

- 在 if 分支分析前，用当前位置的常量表判定条件真值。
- 条件为真：仅分析 consequence。
- 条件为假：仅分析 alternative；无 else 时保持当前常量。
- 条件未知：保留原有 if/else 分支合并。
- 条件支持括号、`!`、局部常量标识符与字面量。

## 验收

- [x] `if (true)` 只应用 then 分支常量。
- [x] `if (false)` 只应用 else 分支常量。
- [x] 未知条件仍合并两分支。
- [x] 未选中分支内的赋值不影响后续常量。
- [x] 规则 fixture、taint 单测与全量门禁通过。

## 排除项

- 循环、switch、try 的分支可达性。
- 跨函数条件常量与复杂谓词求值。
