---
status: done
---

# Task Spec：循环/switch/try 分支合并

task-id: `rule-fp-fn-p14`

## 范围

- 循环：
  - `while` / `for` / `for-in` 可能零次执行，循环后与循环体状态合并。
  - `do-while` 至少执行一次，采用循环体状态。
- switch：
  - 每个 case/default 从 switch 前状态分析并合并。
  - 无 default 时并入“无匹配”路径。
  - 不建模精确 fallthrough。
- try：
  - try 与 catch 状态合并；finally 在合并结果上执行。
  - 无 catch 时采用 try 正常路径。
- 合并规则：两路径常量相同保留，否则移除。

## 验收

- [x] while 可能跳过时合并为未知，两分支身份命中。
- [x] while 两路径常量相同时保留并裁剪。
- [x] do-while 应用至少一次循环体。
- [x] switch 不同 case 合并为未知；相同 case 保留。
- [x] try/catch 不同常量合并为未知；finally 覆盖结果。
- [x] taint 单测、规则 fixture 与全量门禁通过。

## 排除项

- 循环不动点/精确迭代次数。
- switch fallthrough 精确路径与循环 break/continue 的路径敏感求解。
