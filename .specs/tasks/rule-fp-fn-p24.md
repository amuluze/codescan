---
status: done
---

# Task Spec：规则误报审查 P24

task-id: `rule-fp-fn-p24`

## 背景

自扫描与规则复核确认下一批高优先级误报：

- `metric/unreachable-code` 把 Rust/Java 的 `line_comment`/`block_comment` 当作不可达语句。
- Rust/Go 的 `panic!` 检测递归扫描整个表达式语句，`for`/`match` 内的 `panic!` 会把外层控制流误判为终止，导致其后的正常代码被报为不可达。
- `text/http-url` 把 XML/SVG 命名空间、Apple plist DTD、MPL 许可证 URL 当作可拦截的明文网络端点。

## 范围

- unreachable metric 跳过 `comment` / `line_comment` / `block_comment`。
- panic/reachable 终止检测只看表达式语句的直接表达式，不递归进入控制流。
- 文本规则新增可选 `text_exclude` 正则；`text/http-url` 排除命名空间与已知许可证/本地 IPC URL。
- 补 Rust 注释、嵌套 panic、SVG 命名空间 fixture。

## 验收

- [x] Rust/TS 注释不再触发 `metric/unreachable-code`。
- [x] `for` 内 `unreachable!()` 不把循环后的代码判为不可达。
- [x] SVG `xmlns="http://www.w3.org/2000/svg"`、Apple DTD、MPL URL 不触发 `text/http-url`；普通 `http://api.example.com` 仍命中。
- [x] 全量门禁通过。

## 排除项

- `rust/fs-read-unbounded` 对内部配置文件的告警仍保留（需要按路径/大小建模）。
- 注释内容中的真实 TODO/FIXME 仍由文本规则报告。
