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

- 期望命中行 = 标注注释所处连续块指向的**下一个非空且非标注行**：`// ruleid: <id>`（TS/TSX/JS/JSX/Java/Go/Rust）或 `# ruleid: <id>`（Python）。同一行被多条规则命中时，垂直堆叠多条标注即可（都指向同一目标行）。
- Runner 对每个 fixture 目录运行**全部启用规则**；期望多重集与实际命中的 `(规则 id, 文件, 起始行)` 三元组必须**计数一致**：标注了没命中 = 漏报；未标注的命中（含其他规则、度量的命中）= 过度匹配，同样失败。
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


## 污点规则（v2.1，过程内近似）

```yaml
  - id: ts/taint-eval-param
    language: typescript          # 必须具体语言（不允许 "*"）
    dimension: security
    severity: error
    message: 不可信参数可能流入 eval()（过程内污点近似）
    taint:
      sources:                    # 必填，≥1；产生污点的结构模式
        - |
          (function_declaration
            parameters: (formal_parameters
              (required_parameter (identifier) @match))) @match
      sinks:                      # 必填，≥1；汇聚点（发现 span = 该命中）
        - |
          (call_expression
            function: (identifier) @fn (#eq? @fn "eval")) @match
      sanitizers:                 # 可选；命中范围内的污点视为已清洗
        - |
          (call_expression
            function: (identifier) @fn (#eq? @fn "parseInt")) @match
```

语义与限制（Domain Spec §4a，v4）：

1. **同文件过程间**：按调用名索引本文件函数（`f(...)`、`obj.f(...)` 取末段名）；位置参数映射形参；**重名函数取并集**过近似；返回值可携带函数内部 source 的污点。跨文件、堆别名、字段/路径敏感不追踪；变量名扁平化（`obj.prop` 归约为标识符）。
2. **摘要不动点**：每函数每形参的「可达 sink」布尔摘要做不动点迭代，支持互递归；收敛后才做最终收集（深度上限 6，调用环剪枝）。
3. **传播**：赋值/声明把 RHS 污点传给 LHS 标识符；source 直接出现在 sink 内也算命中；调用实参污点经形参映射进入被调函数。
4. **sanitizer**：RHS 与 sanitizer 范围重叠则不传播；sink 内落在 sanitizer 范围内的标识符不算污点。
5. `@match` 多捕获时取**跨度最小**者作为端点；sink 判定同时要求节点种类与字节范围相等（避免 block/expression_statement/call 共享范围重复报告）。
6. 发现证据：`flows` = 源头位置列表（跨函数时合并调用链上的多个源头），UI 与 SARIF `properties.flows` 展示；同一 sink 的多条路径合并为一条发现。
7. 消息口径：只能表述「不可信数据可能到达」的同文件近似，不得声称跨文件或堆敏感证明。

## 忽略规则（扫描范围）

扫描范围由四层忽略源共同决定（Domain §3.1）：

1. 工作区 `.gitignore`（无需 `.git` 存在也生效）；
2. 工作区根 `.codescanignore`（gitignore 语法）；
3. 应用级全局忽略文件 `~/.codescan/ignore`（gitignore 语法，作用于任何工作区）；
4. 内置默认排除目录：`node_modules`、`target`、`dist`、`build`、`.venv`、`__pycache__`、`.next`、`coverage`。

被忽略文件不逐条出事件，统一计入 `IgnoredSummary` 数量。
