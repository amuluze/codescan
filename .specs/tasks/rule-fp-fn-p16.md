---
status: done
---

# Task Spec：带标签 break/continue

task-id: `rule-fp-fn-p16`

## 范围

- 控制流出口记录可选标签：`break label` / `continue label`。
- 带标签语句：
  - 与自身标签匹配的 break 转为标签语句后的正常路径。
  - 匹配的 continue 保守视为标签语句后的路径。
- 标签向外层传播：
  - 内层循环/switch 只消费无标签出口。
  - 外层 labeled statement 消费匹配标签。
  - 其余标签继续向更外层传播。
- 标签块中的 break 后续语句不参与常量分析。

## 验收

- [x] `break outer` 从内层循环退出外层 labeled loop。
- [x] `continue outer` 从内层循环重启外层 labeled loop。
- [x] labeled block 的 break 跳过块内后续赋值。
- [x] 原有 switch fallthrough、无标签 break/continue 测试保持通过。
- [x] 规则 fixture、taint 单测与全量门禁通过。

## 排除项

- 跨函数/异常控制流的标签传播。
- 精确 continue 目标循环的后续迭代状态。
