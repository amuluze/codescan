---
status: done
---

# Task Spec：if/else 分支常量合并

task-id: `rule-fp-fn-p12`

## 背景

局部常量按线性源码顺序处理时，if/else 后只保留最后写入值：

```ts
let flag;
if (cond) { flag = true; } else { flag = false; }
return flag ? a : b; // 应为未知，取并集
```

## 范围

- 常量分析识别 `if_statement`：
  - 分别用当前常量表分析 consequence / alternative。
  - 合并：两分支值相同则保留；不同、缺失或非字面量则移除。
  - 无 else 分支时，用当前值参与合并。
  - 条件表达式中的标识符按当前位置记录。
- 分支内嵌套 if 继续递归处理。
- 分支后的条件裁剪与 `||` / `??` 使用合并结果。

## 验收

- [x] if/else 两分支写相同常量 → 保留并裁剪。
- [x] if/else 写不同常量 → 合并为未知，返回双分支并集。
- [x] 无 else 时与当前值合并为未知。
- [x] 嵌套分支与既有常量传播测试保持通过。
- [x] 规则 fixture、taint 单测与全量门禁通过。

## 排除项

- 循环/switch/try 分支合并。
- 条件值驱动的分支可达性裁剪；if 条件本身未知时保守合并。
