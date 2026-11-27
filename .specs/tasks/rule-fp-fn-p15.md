---
status: done
---

# Task Spec：switch fallthrough 与 break/continue 路径

task-id: `rule-fp-fn-p15`

## 范围

- switch：
  - 从每个 case/default 入口顺序执行后续 case body，直到 break。
  - break 终止 fallthrough 并作为 switch 后正常路径。
  - 无 default 时并入无匹配路径。
- break/continue：
  - 块语句按控制流状态分析，break/continue 后续语句不执行。
  - 循环收集正常、break、continue 出口状态；循环后合并可能路径。
  - 嵌套 if 中的 break/continue 同样传播到最近循环/switch。
- 常量合并仍为相同保留、不同移除。

## 验收

- [x] switch fallthrough 串联 case 赋值，直到 break。
- [x] do-while 中 if-break 产生 break 与非 break 两条路径。
- [x] do-while 中 if-continue 产生 continue 与非 continue 两条路径。
- [x] 原有 if/loop/switch/try 合并测试保持通过。
- [x] 规则 fixture、taint 单测与全量门禁通过。

## 排除项

- 任意 goto/标签与异常控制流。
- 循环不动点、精确迭代次数与带标签 break。
