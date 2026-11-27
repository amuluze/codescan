---
status: done
---

# Task Spec：规则漏报修复 P2

task-id: `rule-fp-fn-p2`

## 背景

审查确认导入别名/全限定名导致的漏报：

- `ts/child-process-string` 只匹配 `child_process.exec`，漏掉 `require` 别名与解构导入。
- `py/weak-hash` 只匹配 `hashlib.md5`，漏掉 `from hashlib import md5` 与 `import hashlib as h`。
- `py/os-system` 只匹配 `os.system`，漏掉 `from os import system` 与 `import os as o`。
- `java/weak-message-digest` 漏掉 `java.security.MessageDigest`。
- JS/TS `no-eval` 漏掉 `window.eval`。
- `rust/taint-fs-write-param` 漏掉 `std::fs::write`。

## 范围

- 新增 pattern 规则可选 `imports` 正则条件：文件 import 声明至少命中一条才应用；覆盖 TS/JS `import`/`require`、Python、Java、Go、Rust。
- 新增 `imported_symbols`：重命名导入（`exec as runExec`、`md5 as weak_md5`）可满足对应 metavariable。
- 上述三条规则使用 import 条件，扩展为任意对象属性/裸函数调用。
- `java/weak-message-digest` 放宽接收者并保留 `MessageDigest$` 约束。
- `ts|tsx|js|jsx/no-eval` 增加 `window/globalThis/self.eval`。
- `rust/taint-fs-write-param` sink 接受 `std::fs::write`。
- 补别名/解构/全限定名正例与无 import 负例。

## 验收

- [x] `require("child_process")` 别名 `.exec`、裸 `exec`、`exec as runExec`、`exec: runAlias` 均命中；无 import 文件不命中。
- [x] `from hashlib import md5`、`import hashlib as h`、`md5 as weak_md5` 命中；无 import 的 `md5(...)` 不命中。
- [x] `from os import system`、`import os as o`、`system as run_system` 命中；无 import 的 `system(...)` 不命中。
- [x] `java.security.MessageDigest.getInstance("MD5")` 命中。
- [x] `window.eval` 命中，JS/TS 四种语言变体均覆盖。
- [x] `std::fs::write` 污点流命中。
- [x] 规则 fixture 与 `task check` 全绿。

## 排除项

- 运行时类型/值解析。
- 将 import 条件扩展到 taint/metric/text 规则。
