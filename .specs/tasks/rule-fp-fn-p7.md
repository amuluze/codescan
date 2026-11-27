---
status: done
---

# Task Spec：跨函数字段写别名传播

task-id: `rule-fp-fn-p7`

## 背景

taint 摘要只覆盖“形参→sink/返回值”，调用方无法看到被调函数对形参对象字段的写入：

```ts
function fill(o, v) { o.value = v; }
function f(input) { const o = {}; fill(o, input); eval(o.value); }
```

## 范围

- 为每个函数提取 `param.field ← param/内部污点` 的写效果摘要。
- 支持局部别名（`const x = o; x.value = v`）与嵌套字段（`o.inner.value = v`）。
- 在调用点将效果回放到调用方实参对象；读取字段时继续命中。
- 仅在目标为形参派生对象时记录效果；字面量写入不传播污点。
- Domain §4b 更新为允许形参字段写效果摘要，仍不传播完整堆图。

## 验收

- [x] 直接字段写跨函数传播。
- [x] 局部别名后的字段写跨函数传播。
- [x] 嵌套字段跨函数传播。
- [x] 字面量字段写不产生污点。
- [x] taint 单测、规则 fixture、全量门禁通过。

## 排除项

- 完整对象堆图/形状摘要与跨过程对象身份等价。
- 返回值对象形状摘要之外的引用逃逸分析。
