---
status: done
---

# Task Spec：规则误报修复 P1

task-id: `rule-fp-fn-p1`

## 背景

续 P0 审查，确认三条 P1 误报：

- `yaml.load(text, yaml.SafeLoader)` 位置参数安全加载器未排除。
- `java/read-object` 按方法名匹配，隐式 `readObject()` 自调用被误报。
- `go/unsafe-template` 按 `template.*` 调用匹配，无法区分 `text/template` 与 `html/template`。

## 范围

- `py/yaml-load-without-loader` 增加位置参数 SafeLoader/CSafeLoader 排除。
- `java/read-object` 要求显式接收者，降为 warning，消息改为需确认接收者类型。
- `go/unsafe-template` 改为匹配 `text/template` import；`html/template` 不再命中。
- 补充安全位置参数、隐式 readObject、html/template 负例。

## 验收

- [x] `yaml.load(text, yaml.SafeLoader)` 与 `yaml.load(text, yaml.CSafeLoader)` 不再命中。
- [x] 隐式 `readObject()` 不再命中；`in.readObject()` 仍命中。
- [x] `import "html/template"` 不再命中；`import "text/template"` 仍命中。
- [x] 规则 fixture 全量通过。
- [x] `task check` 全绿。

## 排除项

- 导入别名/全限定名漏报。
- 运行时接收者类型解析。
