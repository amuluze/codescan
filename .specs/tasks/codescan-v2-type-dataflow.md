---
status: done
---

# Task Spec：Codescan v2 类型层与 Go 数据流

task-id: `codescan-v2-type-dataflow`

## 背景

v1 结构规则无法处理两类边界：Java 通过继承间接实现 `X509TrustManager` 的类，以及 Go 中经变量/参数/拼接到达 `net/http` 的 `http://` URL。本任务按 Domain §4a/§4c 引入最小 v2 层，不扩展为通用类型系统或全量数据流。

## 范围

- **Java 类型继承规则**
  - 新规则 kind `inheritance`：`target`、`methods`、`empty_only`、`indirect_only`。
  - 提取 Java `class_declaration`、`interface_declaration`、匿名类体、`extends`/`implements`、class import/通配包 import、package 与类内方法事实。
  - 按 package/import/全限定名解析同工作区类，计算继承闭包。
  - `java/trust-all-certs-inherited` 仅报告非直接实现目标、空 `checkClientTrusted/checkServerTrusted` 的类；直接/匿名实现继续由 AST 规则负责。
- **Go 明文 URL 数据流**
  - `go/plaintext-http` 从 AST 规则升级为 taint 规则：source = 含 `http://` 的字符串字面量，sink = `http.Get/Head/Post/PostForm`。
  - 同文件函数内赋值、`const`/`var`、package 级 `var`/`const` 初始化进入数据流状态。
  - 移除 info 级 `go/http-client-call`（避免与已证明的明文命中重复，也避免对未知 URL 做无证明推断）。
- 文档：Domain §4c、`docs/rules.md`/`docs/en/rules.md`、README、CHANGELOG。

## 验收标准

- **A1 Java 跨文件间接继承**：同 package 与 import 跨 package 的 `extends Base` 场景命中；直接实现/匿名实现不被继承规则重复命中。
- **A2 Java 无证明不报**：未实现目标类型的同名空方法、无法解析的父类型不产生继承发现。
- **A3 Go 数据流**：变量赋值的 `http://` 到达 `http.Get(url)` 命中；`https://` 不命中；直接字面量与拼接字面量命中。
- **A4 确定性**：类型事实按文件顺序收集、类按稳定 id 分析，最终发现走统一指纹/排序。
- **A5 回归**：Clippy `-D warnings`、引擎单测/集成/规则套件、桌面 headless、前端 typecheck/vitest/build、notices 全绿。

## 结论（2026-11-26，已实施）

- 新增 `types` 模块（Java 类型事实 + 继承闭包）与 `inheritance` 规则 kind；`java/trust-all-certs-inherited` 覆盖同文件/跨文件/跨 package import/接口继承/匿名类体的间接实现。
- `go/plaintext-http` 改为 taint 规则，验证变量、`const`/`var`、package 级初始化与直接字面量到达 `net/http`；移除 `go/http-client-call`。
- 规则总数 90（Java 12、Go 10）；security 76 / business-logic 3 / quality 11。
- 验证：引擎 51 单测 + 19 集成 + 规则套件全量；桌面 25 headless；前端 39 Vitest；Clippy 0；`task check` 通过。
- 边界：Java 覆盖 class/interface/匿名类体的直接 extends/implements 闭包；不处理反射/代理/泛型/动态派发。Go 证明同文件字符串字面量数据流（含 const/var 与 package 级初始化），不证明运行时 URL 或跨文件全局。
