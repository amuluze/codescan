---
status: done
---

# Task Spec：规则误报修复 P0

task-id: `rule-fp-fn-p0`

## 背景

基于真实样例审查，确认三条 P0 误报：

- `postMessage` 通配 origin 规则匹配任意字符串参数，误报 `postMessage(data, "https://…")`。
- `py/taint-os-system-request` 把任意 `.args` 当作 `request.args`，误报普通配置对象。
- `go/tls-insecure-skip-verify` 匹配任意 `InsecureSkipVerify: true` 字段，误报自定义结构体。

## 范围

- `ts|tsx|js|jsx` 的 `no-postmessage-wildcard` 锚定第二个实参为 origin。
- `py/taint-os-system-request` source 限定 `request.args`。
- `go/tls-insecure-skip-verify` 增加 `inside tls.Config` 过滤。
- 四个 JS/TS 变体补充安全 origin 负例；Python/Go 补充非目标负例。

## 验收

- [x] `postMessage("*", "https://example.com")` 不再命中；`postMessage(data, "*")` 仍命中。
- [x] `os.system(config.args)` 不再命中 taint 规则；`os.system(request.args.get(...))` 仍命中。
- [x] 自定义结构体 `InsecureSkipVerify: true` 不再命中；`tls.Config{InsecureSkipVerify: true}` 仍命中。
- [x] 规则 fixture 全量通过。
- [x] `task check` 全绿。

## 排除项

- P1：`yaml.load` 位置参数、`java/read-object`、`go/unsafe-template`。
- 导入别名/全限定名漏报；需后续 import-aware 规则过滤。
