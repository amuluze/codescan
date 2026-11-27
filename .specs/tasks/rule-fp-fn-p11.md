---
status: done
---

# Task Spec：局部变量赋值常量传播

task-id: `rule-fp-fn-p11`

## 背景

条件值为局部常量时，身份分支仍取并集：

```ts
const flag = true;
return flag ? a : b; // 只应关联 a
```

## 范围

- 在函数范围内收集简单常量赋值：
  - `const/let/var x = 字面量`
  - `x = 字面量`
  - 覆盖 `true/false`、`null/undefined`、数字、字符串、对象/数组字面量、`!` 取反。
- 按语句顺序记录表达式位置的常量快照：后写覆盖前值，条件使用其所在位置的当前值；非字面量赋值从常量表移除。
- 条件裁剪与 `||` / `??` 使用这些常量。
- 不跨函数/模块传播常量。

## 验收

- [x] `const flag = true; flag ? a : b` 只关联 `a`。
- [x] 后续 `flag = false` 使返回只关联 `b`。
- [x] 条件先读取 `true`、之后再改 `false` 时，按表达式位置取当时的值。
- [x] `const flag = null; flag ?? b` 只关联 `b`。
- [x] 条件未知时仍取分支并集。
- [x] 规则 fixture、taint 单测与全量门禁通过。

## 排除项

- 跨函数常量传播与 `const` 对象属性常量。
- 控制流合并/可达性求解。
