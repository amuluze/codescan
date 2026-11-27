---
status: done
---

# Task Spec：P0 条件摘要与导入绑定

task-id: `rule-fp-fn-p19`

## 范围

- 跨函数效果/返回身份加入调用点 guard：
  - 条件字段写只在实参条件满足时回放。
  - 不可达返回分支不产生身份；条件返回按实参常量裁剪。
- `ts/child-process-string`：
  - member 规则要求接收者为 `child_process` 或其模块别名。
  - bare 规则继续覆盖解构导入；拆分为 `-member` 新规则。
- `py/taint-os-system-request-alias`：
  - `imports` + `imported_symbols` 绑定 request 别名。
  - source 端点捕获对象名，只有别名映射到 request 才生效。
- README 规则统计更新。

## 验收

- [x] `maybe(o, input, false)` 不传播字段污点；true 传播。
- [x] `if (false) return a` 不建立 a 身份；条件 return 按 false 实参裁剪。
- [x] `db.exec` 不再被 child_process 规则误报；`cp.exec` 仍命中。
- [x] Flask `req.args` 命中；同一文件 `config.args` 不命中。
- [x] 规则套件、taint 单测与全量门禁通过。

## 排除项

- 运行时条件值求解；guard 目前使用实参字面量真值。
- 完全的跨函数堆/对象身份等价。
