# Codescan 规则编写指南

内置规则按语言分文件存放于 `crates/engine/rules/`：`builtin.yaml`（度量 + 通用文本规则）、`typescript.yaml`（TS / TSX / JS / JSX 分别声明规则）、`python.yaml`、`java.yaml`、`go.yaml`、`rust.yaml`。规则集随引擎版本化（Domain Spec §4.4）。

桌面端规则页可直接编辑 `~/.codescan/rules/custom.yaml`、校验编译、保存，并对样例代码运行仅包含自定义规则的测试。内置规则只读。

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

1. **`@match` 捕获决定 span，且为强制项**。pattern、`filters.not`/`filters.inside` 与 taint 的每条 query 都必须包含 `@match`；编译期会拒绝缺失 `@match` 的规则。`@match` 有多个时取跨度最小者；请显式写 `@match` 并尽量落在最小有意义节点上。
2. **query 谓词**：`#eq?` / `#match?` 等文本谓词由引擎原生执行，优先在 query 内解决精确匹配；`metavariables` 是补充手段。
3. **span 尽量小**：命中整行还是整个调用，决定 UI 展示与指纹粒度，选最小的有意义节点。
4. `filters.not` / `filters.inside` 的 query 同语言、同样要求捕获（推荐 `@match`）。

### 导入条件（可选，pattern 规则）

`imports` 为正则列表，仅在文件的 import 声明至少匹配其中一条时应用规则：

```yaml
  - id: ts/child-process-string
    # ...
    imports:
      - 'child_process'
```

- 适用范围：TypeScript/JavaScript 的 `import` 与 `require(...)`、Python 的 `import`/`from`、Java `import`、Go `import`、Rust `use`。
- 用于覆盖别名导入与解构导入，同时避免把同名普通函数误报为库调用。
- `imported_symbols` 声明库的原始符号名；重命名导入（`exec as runExec`、`md5 as weak_md5`）可满足对应 metavariable：

```yaml
    imports:
      - 'child_process'
    imported_symbols:
      - exec
      - execSync
```

- Go 包别名映射到包 basename（如 `import ex "os/exec"` 的 `ex → exec`）；Rust `use ... as`（含嵌套 `use std::{fs as f}`）映射到完整路径（如 `use std::fs as f` 的 `f → std::fs`），因此 `imported_symbols` 需写完整路径。
- `imports` 可用于 `pattern` 与 `taint` 规则，用于按文件 import 限定端点；`imported_symbols` 仅用于 `pattern` 规则。`text`、`metric` 规则不能声明。
- `no_shadow: true`：文件内存在同名非 `require(...)` 本地定义时丢弃该 pattern 规则命中；用于避免同名局部函数/变量被当作导入符号。
- Java `field_receiver: true`：规则必须捕获名为 `obj` 的接收者；仅当该名称是匹配点所在类中 `ObjectInputStream` 字段时命中。用于字段接收者规则，避免把方法参数或局部变量误判为字段。

### 文本规则（正则）

`language: "*"`，`text:` 为 Rust `regex` 语法。**不支持 lookahead/lookbehind**（与 PCRE 差异最大处），请用字符类与锚点改写。对工作区全部非二进制文本文件逐文件扫描，含注释、字符串、隐藏文件（如 `.env`、`.config/`）与未知扩展名；`.git` 始终排除。AST/污点/度量只对已识别语言执行。


### 度量（内置，勿在语言文件中重复声明）

`metric/cyclomatic-complexity`（阈值 25）、`metric/function-length`（200 行）、`metric/nesting-depth`（5）、`metric/duplicate-block`（窗口 12 行、≥2 处）、`metric/unreachable-code`。阈值改动在 `builtin.yaml` 的 `options` 中进行；自定义规则可声明同 kind 的 metric 规则，发现的 id/severity/dimension/message 以该规则声明为准。

## 规则包（本地 MVP）

规则包安装于 `~/.codescan/packs/<id>/<version>/`，目录内必须有 `pack.yaml`：

```yaml
id: com.example.java-security
version: 1.0.0
engine_min: 1.5.0       # 可选
engine_max: 2.0.0       # 可选
description: Java security pack
rules:
  - rules.yaml
```

桌面规则页与 CLI `codescan pack` 均可管理规则包；包规则只读，来源显示为 `<id>@<version>`。安装时写入内容哈希 `.integrity`，加载时校验；篡改内容会使规则包失效。扫描历史会记录已启用规则包 id/version/integrity/signed_by；CLI `codescan scan` 自动加载已安装包。安装与扫描时校验规则编译与 id 冲突。

### 签名与信任

规则包可用 Ed25519 分离签名 `.signature`：

```json
{
  "algorithm": "ed25519",
  "key_id": "<sha256(公钥) hex>",
  "signature": "<base64 64 字节签名>"
}
```

- 签名字节为 `codescan-pack-integrity:v1:<内容哈希>`；内容哈希覆盖 `pack.yaml` 与 manifest 声明的规则文件，不覆盖 `.signature` / `.integrity`。
- 信任公钥存放在 `<data_root>/trusted-keys/<key_id>.pub`，内容为 32 字节原始公钥。
- 未配置信任公钥：允许未签名包；已签名包因无法验证而拒绝。
- 已配置信任公钥：所有包必须由受信任公钥签名；未签名包拒绝。
- 签名格式错误、密钥未受信任、签名不匹配均拒绝安装与扫描。
- CLI 流程：`pack keygen --out keys` → `pack trust add keys/public.key` → `pack sign ./my-pack --key keys/private.key` → `pack install ./my-pack`。
- 私钥只用于本地签名，不进入规则包、扫描历史或列表输出。远程下载/注册表仍不在范围内。

## 规则质量门槛

- 每条规则至少一个正例 fixture；负例与正例同目录，runner 的精确多重集判定会拒绝未标注命中。
- 消息只表述结构构造，不声称可利用性；taint 规则使用“可能到达”口径。
- severity/dimension 由规则声明；规则 id 语义变更必须伴随版本推进。
- 新增规则不得让既有 fixture 产生额外命中；规则套件全量通过后才合并。

## Fixture（规则单测，A2 验收）

目录：`crates/engine/tests/fixtures/rules/<rule-id>/`，按文件扩展名识别语言。判定语义（Task Spec「规则 fixture 格式」）：

- 期望命中行 = 标注注释所处连续块指向的**下一个非空且非标注行**：`// ruleid: <id>`（TS/TSX/JS/JSX/Java/Go/Rust）或 `# ruleid: <id>`（Python）。同一行被多条规则命中时，垂直堆叠多条标注即可（都指向同一目标行）。
- Runner 对每个 fixture 目录运行**全部启用规则**；期望多重集与实际命中的 `(规则 id, 文件, 起始行)` 三元组必须**计数一致**：标注了没命中 = 漏报；未标注的命中（含其他规则、度量的命中）= 过度匹配，同样失败。
- 因此 fixture 必须「干净」：不触发无关规则与度量（函数保持短小；不引入重复 12 行块；不写真实密钥样式之外又撞上其他文本规则的内容）。

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

语义与限制（Domain Spec §4a/§4b，v5）：

1. **跨文件边界**：同语言**显式导入**可跨文件——TS/JS/TSX/JSX（相对路径具名/默认导入）、Python（`from .mod import name` 与 package 相对导入）、Go（同目录同包）、Java（`import a.b.C` 类导入 + `static` 成员导入，类→文件按 `package` + 文件名）、Rust（`use crate::/super::/self::…`、`as` 别名、`use …::{a, b}`，模块→文件按 crate `src/` 根）；带 `crate::`/`super::`/`self::` 前缀的限定调用可解析。未显式导入的同名函数不跨文件；Java 实例方法、TS 命名空间导入、Rust 外部 crate 不在解析范围。重名函数取并集过近似。
2. **同文件过程间**：调用名取末段（`f(...)`、`obj.f(...)`）；位置参数映射形参；返回值可携带函数内部 source 的污点。布尔摘要（形参→可达 sink / 返回值→形参依赖 / 返回值内部污点）做不动点迭代，支持互递归；最终收集深度上限 6、调用环剪枝。
3. **字段敏感**：污点状态按访问路径 `base.field…` 表达；对象字面量按字面键展开；未知索引访问（`obj[k]`）通配；读取整个对象视为包含其字段污点；不同字段互不污染。
4. **对象组别名**：`a = b`（裸标识符）使 a 加入 b 的对象组，经任一别名的字段写入对该组可见；重新赋值以新组强更新。非全量 points-to/形状敏感，仍是已声明近似。
5. **sanitizer**：表达式/实参落入 sanitizer 即切断；sink 内落在 sanitizer 范围内的标识符不算污点。
6. **别名/点/通配导入**：source/sink/sanitizer 可按函数名或任意包名匹配，再用 `imports` gate 限定文件；例如 `s.Atoi`、`Atoi` 都可作为 sanitizer。
7. **跨函数字段写与身份**：保留 `param.field ← param/内部污点` 与形参对象身份集合摘要（条件/联合返回按可判定条件（含局部常量赋值、if/else/循环/switch/try 分支合并（含 switch fallthrough、带标签/无标签 break/continue 及 labeled continue 下一轮状态；标签不跨函数传播）与可判定条件裁剪），未知取分支并集），并在调用点回放；例如 `fill(o, v){ o.value = v }` 和 `id(o){ return o }` 后调用方读取 `o.value` 可继续命中；不传播完整对象堆图。
6. `@match` 多捕获时取**跨度最小**者；sink 判定同时要求节点种类与字节范围相等。
7. 发现证据：`flows` = 源头位置列表（跨文件/跨函数合并多条路径），同一 sink 合并为一条发现；SARIF `properties.flows` 同步。
8. 消息口径：只能表述「不可信数据可能到达」的近似，不得声称跨文件完备性或堆敏感证明。

## 继承规则（v2 类型层，Java）

```yaml
  - id: java/trust-all-certs-inherited
    language: java                # 当前仅支持 java
    dimension: security
    severity: warning
    message: 通过继承间接实现 X509TrustManager 的空 check 方法（需人工确认）
    inheritance:
      target: X509TrustManager    # 目标接口/父类型（简单名或全限定名）
      methods:                    # 类内要检查的方法名
        - checkClientTrusted
        - checkServerTrusted
      empty_only: true            # 仅空方法体
      indirect_only: true         # 只报非直接实现，避免与直接实现 AST 规则重复
```

语义与限制（Domain §4c）：

1. 解析同一工作区内的 `class_declaration`、`interface_declaration`、匿名类体、`extends`/`implements` 与 class import/通配包 import，按 package/import/全限定名解析类型；`indirect_only` 只报告继承闭包可达、但自身未直接实现 target 的类。
2. 直接实现与匿名直接实现由 AST 规则覆盖；无法解析的外部父类型、反射/代理、泛型与动态派发不做推断；无证明则不报。
3. span = 方法声明，证据 = 方法源文本；指纹、degraded 与排序沿用通用发现模型。

## 忽略规则（扫描范围）

扫描范围由四层忽略源共同决定（Domain §3.1）：

1. 工作区 `.gitignore`（无需 `.git` 存在也生效）；
2. 工作区根 `.codescanignore`（gitignore 语法）；
3. 应用级全局忽略文件 `~/.codescan/ignore`（gitignore 语法，作用于任何工作区）；
4. 内置默认排除目录：`node_modules`、`target`、`dist`、`build`、`.venv`、`__pycache__`、`.next`、`coverage`、`.git`。

隐藏文件与隐藏目录纳入扫描（如 `.env`、`.config/`），未知扩展名的文本文件仍执行文本规则；`.git` 始终排除。被忽略文件不逐条出事件，统一计入 `IgnoredSummary` 数量。
