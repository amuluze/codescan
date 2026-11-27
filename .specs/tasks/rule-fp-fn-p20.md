---
status: done
---

# Task Spec：P1 tls 别名与 readObject 类型关联

task-id: `rule-fp-fn-p20`

## 范围

- `go/tls-insecure-skip-verify`：
  - 从 `keyed_element + inside` 改为 composite literal 整体匹配。
  - 捕获 package/type，使用 `imports` + `imported_symbols` 解析 `crypto/tls` 别名。
  - 保留自定义结构体负例。
- `java/read-object`：
  - 要求命中位于包含 `ObjectInputStream` 形参或局部变量的方法内。
  - 保留显式接收者要求。
  - 移除 XMLDecoder.readObject 的误报。
- 补充 Go 别名正例与 Java 自定义接收者负例。

## 验收

- [x] `import tc "crypto/tls"; &tc.Config{InsecureSkipVerify: true}` 命中。
- [x] 自定义 `InsecureSkipVerify` 结构体不命中。
- [x] Java 自定义 `other.readObject()` 不命中。
- [x] ObjectInputStream 方法内 `in.readObject()` 仍命中。
- [x] 规则套件与全量门禁通过。

## 排除项

- Java 字段/跨方法接收者类型推断。
- Go 未导入包的完全限定类型解析。
