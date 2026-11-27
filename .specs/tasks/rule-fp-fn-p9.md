---
status: done
---

# Task Spec：条件/联合返回对象身份集合

task-id: `rule-fp-fn-p9`

## 背景

单一对象身份无法覆盖条件/联合返回：

```ts
function choose(a, b, flag) { return flag ? a : b; }
const c = choose(a, b, flag);
c.value = input;
// a.value / b.value 都应命中
```

## 范围

- 返回表达式收集值分支身份集合：
  - TS/JS 三元表达式 `cond ? a : b`。
  - TS/JS `a || b`、`a ?? b`。
  - 嵌套括号/嵌套条件递归收集。
- 不把条件表达式本身的参数（如 `flag`）计入身份集合。
- 调用点将结果与所有分支实参对象合并；字段写同时影响各对象。
- 对象字面量分支不建立身份等价。

## 验收

- [x] 三元返回同时命中两个分支对象。
- [x] nullish/逻辑联合返回同时命中两个分支对象。
- [x] 条件参数不被误当返回对象。
- [x] 对象字面量分支不建立身份等价。
- [x] taint 单测、规则 fixture、全量门禁通过。

## 排除项

- 完整跨过程堆图/形状摘要。
- 分支身份集合的路径敏感过滤（按条件值裁剪）。
