---
status: done
---

# Task Spec：Java 字段 ObjectInputStream 推断

task-id: `rule-fp-fn-p21`

## 范围

- `java/read-object` 的 inside 过滤增加类字段分支：
  - `field_declaration` 类型为 `ObjectInputStream` 或全限定名。
  - 方法内使用该字段的 `readObject()` 命中。
- 保留形参/局部变量分支。
- 保留自定义接收者负例。
- 新增字段读取 fixture，包括同字段的跨方法访问。

## 验收

- [x] `private ObjectInputStream input; input.readObject()` 命中。
- [x] 同字段在其它方法中访问仍命中。
- [x] 无 ObjectInputStream 字段/参数/局部变量的自定义 `other.readObject()` 不命中。
- [x] 规则套件与全量门禁通过。

## 排除项

- 继承字段、接口字段与跨类字段推断。
- 接收者与字段名的精确别名比较（当前为方法/类范围近似）。
