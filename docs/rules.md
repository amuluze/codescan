# Codescan 规则编写指南

内置规则按语言分文件存放于 `crates/engine/rules/`：`builtin.yaml`（度量 + 通用文本规则）、`typescript.yaml`（含 TSX）、`python.yaml`、`java.yaml`、`go.yaml`、`rust.yaml`。规则集随引擎版本化（Domain Spec §4.4）。

## 规则格式

```yaml
rules:
  - id: ts/no-eval                    # 小写字母/数字/`-`/`/`；全局唯一
    language: typescript              # typescript|tsx|javascript|jsx|python|java|go|rust；文本/度量规则必须为 "*"
    dimension: security               # security|business-logic|quality
    severity: error                   # error|warning|info
    message: 使用 eval() 动态执行代码  # 只表述「出现了构造」，不声称可利用性
    pattern: |                        # tree-sitter query，三选一
      (call_expression
        function: (identifier) @match (#eq? @match "eval"))
    filters:
      - not:                          # 命中范围「同位或包含」主命中 → 排除
          pattern: |
            (comment) @match
      - inside:                       # 主命中必须被该 query 的命中范围包含
          pattern: |
            (function_declaration) @match
    metavariables:                    # 对捕获文本的正则约束；捕获缺失也排除
      - capture: match
        regex: "^eval$"
```

要点：

1. **`@match` 捕获决定 span**。没有 `@match` 时取全部捕获的最小/最大范围；完全没有捕获的 pattern 视为无命中（不会产出发现）。请始终显式写 `@match`。
2. **query 谓词**：`#eq?` / `#match?` 等文本谓词由引擎原生执行，优先在 query 内解决精确匹配；`metavariables` 是补充手段。
3. **span 尽量小**：命中整行还是整个调用，决定 UI 展示与指纹粒度，选最小的有意义节点。
4. `filters.not` / `filters.inside` 的 query 同语言、同样要求捕获（推荐 `@match`）。

### 文本规则（正则）

`language: "*"`，`text:` 为 Rust `regex` 语法。**不支持 lookahead/lookbehind**（与 PCRE 差异最大处），请用字符类与锚点改写。逐文件全量扫描，含注释与字符串。


### 度量（内置，勿在语言文件中重复声明）

`metric/cyclomatic-complexity`（阈值 15）、`metric/function-length`（80 行）、`metric/nesting-depth`（5）、`metric/duplicate-block`（窗口 10 行、≥2 处）、`metric/unreachable-code`。阈值改动在 `builtin.yaml` 的 `options` 中进行。

## Fixture（规则单测，A2 验收）

目录：`crates/engine/rules/tests/<rule-id>/`，按文件扩展名识别语言。判定语义（Task Spec「规则 fixture 格式」）：

- 期望命中行 = 其**紧邻上一行**的标注注释：`// ruleid: <id>`（TS/TSX/JS/JSX/Java/Go/Rust）或 `# ruleid: <id>`（Python）。
- Runner 对每个 fixture 目录运行**全部启用规则**；期望集合与实际命中的 `(规则 id, 文件, 起始行)` 三元组集合必须**精确相等**：标注了没命中 = 漏报；未标注的命中（含其他规则、度量的命中）= 过度匹配，同样失败。
- 因此 fixture 必须「干净」：不触发无关规则与度量（函数保持短小；不引入重复 10 行块；不写真实密钥样式之外又撞上其他文本规则的内容）。

示例：

```ts
// ruleid: ts/no-eval
eval(userInput);
doSomethingSafe();
```

## 各语言可用 node 速查（编写 pattern 时用 `tree-sitter` 官方 node-types 为准）

- TS/TSX/JS/JSX: `call_expression`(fields: function/arguments)、`member_expression`、`identifier`、`string`、`template_string`、`new_expression`、`assignment_expression`、`binary_expression`、`await_expression`、`import_statement`、`jsx_element`。
- Python: `call`(func/arguments)、`attribute`、`identifier`、`string`、`assignment`、`subscript`、`import_statement`、`function_definition`。
- Java: `method_invocation`(name/arguments)、`object_creation_expression`、`field_access`、`string_literal`、`local_variable_declaration`、`assignment_expression`。
- Go: `call_expression`(function/arguments)、`selector_expression`、`identifier`、`interpreted_string_literal`、`short_var_declaration`、`assignment_statement`。
- Rust: `call_expression`(function/arguments)、`macro_invocation`、`field_expression`、`identifier`、`string_literal`、`let_declaration`。

