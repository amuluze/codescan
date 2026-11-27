---
status: done
---

# Task Spec：规则库质量批次 2

task-id: `rule-library-quality-v2`

## 范围

在批次 1 的质量门槛上补充高价值构造规则：

- `java/xml-decoder`：`new XMLDecoder(...)`。
- `java/url-open-connection`：`new URL(...).openConnection()`。
- `rust/unsafe-block`：`unsafe { ... }`。
- `py/flask-debug-run`：Flask `app.run(debug=True)`。

## 验收

- [x] 4 条规则均有正例 fixture；同目录负例不产生未标注命中。
- [x] 规则套件全量通过。
- [x] README 计数更新为 98 条；security 83、quality 12。
- [x] CHANGELOG 记录新增规则。
- [x] `task check` 通过。

## 边界

- Java URL/XMLDecoder 规则是结构构造提示，不证明 SSRF/XXE 可利用性。
- `rust/unsafe-block` 为质量提示，不证明内存安全缺陷。
- Flask 规则只匹配 `app/application.run(debug=True)`。
