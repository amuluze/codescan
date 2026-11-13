# Rule authoring guide (English)

Built-in rules live in `crates/engine/rules/`, one file per language: `builtin.yaml` (metrics + language-agnostic text rules), `typescript.yaml` (covers TSX too), `python.yaml`, `java.yaml`, `go.yaml`, `rust.yaml`. The rule set is versioned with the engine.

The [Chinese version of this document](../rules.md) is the original; this file mirrors it.

## Rule format

```yaml
rules:
  - id: ts/no-eval                    # lowercase letters/digits/`-`/`/`; globally unique
    language: typescript              # typescript|tsx|javascript|jsx|python|java|go|rust; text/metric rules must be "*"
    dimension: security               # security|business-logic|quality
    severity: error                   # error|warning|info
    message: 使用 eval() 动态执行代码  # state only that a construct is present — never exploitability
    pattern: |                        # tree-sitter query; exactly one body kind
      (call_expression
        function: (identifier) @match (#eq? @match "eval"))
    filters:
      - not:                          # exclude hits in the same or an enclosing range of the main hit
          pattern: |
            (comment) @match
      - inside:                       # main hit must be inside the range of this query's hit
          pattern: |
            (function_declaration) @match
    metavariables:                    # regex constraints on captured text; a missing capture also excludes
      - capture: match
        regex: "^eval$"
```

Key points:

1. **The `@match` capture defines the span.** Without `@match` the span is the minimal/maximal range of all captures; a pattern with no capture at all never matches. Always write `@match` explicitly.
2. **Query predicates** such as `#eq?` / `#match?` are executed natively by the engine — prefer them over `metavariables` for exact matching.
3. **Keep spans small**: matching a whole line versus the whole call changes both the UI rendering and the fingerprint granularity. Choose the smallest meaningful node.
4. `filters.not` / `filters.inside` queries use the same language and should also capture (`@match` recommended).

### Text rules (regex)

`language: "*"`, with `text:` in Rust `regex` syntax. **Lookahead/lookbehind are not supported** (the biggest difference from PCRE) — rewrite using character classes and anchors. Text rules scan the whole file, including comments and strings.

### Metrics (built-in; do not redeclare in language files)

`metric/cyclomatic-complexity` (threshold 15), `metric/function-length` (80 lines), `metric/nesting-depth` (5), `metric/duplicate-block` (window of 10 lines, ≥2 occurrences), `metric/unreachable-code`. Thresholds are declared in `builtin.yaml` under `options`.

## Fixtures (rule unit tests)

Directory: `crates/engine/tests/fixtures/rules/<rule-id>/`; the file extension selects the language.

- The expected line is the **next non-empty, non-annotation line** pointed at by the annotation block: `// ruleid: <id>` (TS/TSX/JS/JSX/Java/Go/Rust) or `# ruleid: <id>` (Python). Stack several annotations when several rules hit the same line.
- The runner executes **all enabled rules** per fixture directory. The expected multiset and the actual hits (triples of rule id, file, start line) must match **in count**: an annotated line that did not match is a false negative; an unannotated hit (including hits from other rules or metrics) is an over-match. Both fail the test.
- Fixtures must therefore be *clean*: no unrelated rules or metrics may trigger (keep functions short, avoid repeated 10-line blocks, avoid content that collides with other text rules).

Example:

```ts
// ruleid: ts/no-eval
eval(userInput);
doSomethingSafe();
```

## Node cheat sheet

Consult the official `tree-sitter` node-types for the grammar you target.

- TS/TSX/JS/JSX: `call_expression` (fields: function/arguments), `member_expression`, `identifier`, `string`, `template_string`, `new_expression`, `assignment_expression`, `binary_expression`, `await_expression`, `import_statement`, `jsx_element`.
- Python: `call` (func/arguments), `attribute`, `identifier`, `string`, `assignment`, `subscript`, `import_statement`, `function_definition`.
- Java: `method_invocation` (name/arguments), `object_creation_expression`, `field_access`, `string_literal`, `local_variable_declaration`, `assignment_expression`.
- Go: `call_expression` (function/arguments), `selector_expression`, `identifier`, `interpreted_string_literal`, `short_var_declaration`, `assignment_statement`.
- Rust: `call_expression` (function/arguments), `macro_invocation`, `field_expression`, `identifier`, `string_literal`, `let_declaration`.

## Taint rules (intra-procedural approximation)

```yaml
  - id: ts/taint-eval-param
    language: typescript          # must be a concrete language (never "*")
    dimension: security
    severity: error
    message: untrusted parameter may reach eval() (intra-procedural taint approximation)
    taint:
      sources:                    # required, ≥1; structural patterns that create taint
        - |
          (function_declaration
            parameters: (formal_parameters
              (required_parameter (identifier) @match))) @match
      sinks:                      # required, ≥1; the finding span is the sink hit
        - |
          (call_expression
            function: (identifier) @fn (#eq? @fn "eval")) @match
      sanitizers:                 # optional; taint inside the hit range is considered cleaned
        - |
          (call_expression
            function: (identifier) @fn (#eq? @fn "parseInt")) @match
```

Semantics and limits:

1. **Cross-file boundary**: only **explicit imports** of the same language cross files — TS/JS/TSX/JSX (relative named/default imports), Python (`from .mod import name`, package-relative imports), Go (same directory and package), Java (`import a.b.C` and static member imports; class → file via `package` + file name), Rust (`use crate::/super::/self::…`, `as` aliases, `use …::{a, b}`; module → file via the crate `src/` root). Qualified calls with `crate::`/`super::`/`self::` prefixes resolve. Same-named functions without an explicit import do not cross files; Java instance methods, TS namespace imports, and external Rust crates are out of scope. Same-named functions are approximated by the union of candidates.
2. **Intra-file, inter-procedural**: the call name is the last segment (`f(...)`, `obj.f(...)`); positional arguments map to parameters; a return value can carry taint from sources inside the callee. Boolean summaries (parameter → reachable sink / return value → parameter dependency / taint inside return value) are iterated to a fixed point, supporting mutual recursion; collection depth is capped at 6 and call cycles are pruned.
3. **Field sensitivity**: taint state is expressed as an access path `base.field…`; object literals expand by literal key; unknown index access (`obj[k]`) is a wildcard; reading a whole object implies the taint of its fields; distinct fields do not contaminate each other.
4. **Object-set aliasing**: `a = b` (bare identifiers) adds `a` to `b`'s object set, so a field write through either alias is visible in the set; reassignment strongly updates to a new set. This is not a full points-to/shape-sensitive analysis — it is a declared approximation.
5. **Sanitizers**: an expression or argument inside a sanitizer cuts taint; identifiers inside a sanitizer within the sink do not count as tainted.
6. When `@match` captures several nodes, the **smallest span** is used; sink matching requires both node kind and byte range equality.
7. Evidence: `flows` lists the source locations (multiple paths across files/functions are merged); the same sink yields one finding. SARIF `properties.flows` mirrors it.
8. Message wording: state only the approximation that "untrusted data may reach" the sink — never claim cross-file completeness or heap-sensitive proof.

## Ignore rules (scan scope)

The scan scope is determined by four ignore layers:

1. workspace `.gitignore` (effective even without a `.git` directory);
2. workspace-root `.codescanignore` (gitignore syntax);
3. application-global ignore file `~/.codescan/ignore` (gitignore syntax, applies to any workspace);
4. built-in default directories: `node_modules`, `target`, `dist`, `build`, `.venv`, `__pycache__`, `.next`, `coverage`.

Ignored files are not reported individually; they are counted in the `IgnoredSummary` scan event.
