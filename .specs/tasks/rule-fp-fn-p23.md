---
status: done
---

# Task Spec：规则审查 P0/P1 精化

task-id: `rule-fp-fn-p23`

## 范围

- P0：
  - 局部不可达 sink 不报告（`if (false)`、不可达循环体）。
  - 局部不可达字段写不回放（分支不可达时状态不更新）。
  - 复合条件中的 `&&` guard 参与跨函数效果回放，false 实参不应用字段写/调用效果。
- P1：
  - `ts/child-process-string-member` 增加 `no_shadow`；同名本地模块对象不再被当作导入模块。
  - Java 字段接收者拆分为 `java/read-object-field`，仅接收者为同类 `ObjectInputStream` 字段时命中；`java/read-object` 仍覆盖方法参数/局部变量。
  - switch case 恢复效果回放，并按 case 值等值 guard 排除不匹配调用。
- 补单测/fixture；更新规则数量与文档。

## 验收

- [x] `if(false)` 内 sink 与字段写不产生污点。
- [x] `cond && other` 为 false 时不回放效果。
- [x] 本地 `exec`/`execSync` 不命中 bare 规则；同名本地模块对象不命中 member 规则。
- [x] Java 同名字段接收者精确命中；非 OIS 字段与参数/局部变量按既有规则判定。
- [x] switch case 匹配时回放效果，不匹配时不回放。
- [x] 全量门禁通过。

## 排除项

- 循环条件在**被调函数内部**的 sink 报告仍按函数参数未知处理（需要调用点上下文才能裁剪），保留保守报告。
- 函数参数级同名遮蔽（`no_shadow` 当前为文件级判断）。
- switch 无 `default` 的 fallthrough 精确建模。
- 继承字段与跨类 Java 字段推断。
