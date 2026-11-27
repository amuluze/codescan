# Rule authoring guide (English)

Built-in rules live in `crates/engine/rules/`, one file per language: `builtin.yaml` (metrics + language-agnostic text rules), `typescript.yaml` (separate TS / TSX / JS / JSX declarations), `python.yaml`, `java.yaml`, `go.yaml`, `rust.yaml`. The rule set is versioned with the engine.

The desktop rules page edits `~/.codescan/rules/custom.yaml` directly, validates compilation, saves atomically, and runs a sample-file smoke test with only custom rules enabled. Built-in rules are read-only.

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

1. **The `@match` capture defines the span and is mandatory.** Pattern, `filters.not`/`filters.inside`, and every taint query must contain `@match`; compilation rejects rules without it. When several `@match` captures exist, the smallest span wins. Keep the capture on the smallest meaningful node.
2. **Query predicates** such as `#eq?` / `#match?` are executed natively by the engine — prefer them over `metavariables` for exact matching.
3. **Keep spans small**: matching a whole line versus the whole call changes both the UI rendering and the fingerprint granularity. Choose the smallest meaningful node.
4. `filters.not` / `filters.inside` queries use the same language and should also capture (`@match` recommended).

### Import conditions (optional, pattern rules)

`imports` is a list of regexes; the rule applies only when at least one import declaration in the file matches one of them:

```yaml
  - id: ts/child-process-string
    # ...
    imports:
      - 'child_process'
```

- Covered syntax: TypeScript/JavaScript `import` and `require(...)`, Python `import`/`from`, Java `import`, Go `import`, and Rust `use`.
- This covers aliased and destructured imports while avoiding false positives on unrelated same-named functions.
- `imported_symbols` lists the library's original symbol names; renamed imports (`exec as runExec`, `md5 as weak_md5`) can satisfy the corresponding metavariable:

```yaml
    imports:
      - 'child_process'
    imported_symbols:
      - exec
      - execSync
```

- Go package aliases map to the package basename (for example `import ex "os/exec"` gives `ex → exec`); Rust `use ... as` (including nested `use std::{fs as f}`) maps to the full path (for example `use std::fs as f` gives `f → std::fs`), so `imported_symbols` must use the full path.
- `imports` is allowed on `pattern` and `taint` rules to gate endpoints by file imports; `imported_symbols` is pattern-only. `text` and `metric` rules cannot declare them.
- `no_shadow: true` drops a pattern-rule match when the file defines a same-named local that is not initialized by `require(...)`; use it to avoid treating a local function/variable as an imported symbol.
- Java `field_receiver: true` requires an `obj` capture; the match is kept only when that name is an `ObjectInputStream` field of the enclosing class. This keeps field-receiver rules from confusing method parameters or locals with fields.
- `exclude_paths` is an optional list of path regexes for pattern/text rules, matched against the workspace-relative path; the rule is skipped for files matching any path. Built-in Rust heuristics use it to skip top-level `tests/*.rs`, and `text/todo-debt` uses it to skip docs, task specs, and rule definitions.

### Text rules (regex)

`language: "*"`, with `text:` in Rust `regex` syntax. **Lookahead/lookbehind are not supported** (the biggest difference from PCRE) — rewrite using character classes and anchors. Optional `text_exclude:` is an exclusion regex in the same syntax: a match is not reported when the matched text matches it (for example XML namespaces and local IPC URLs). Text rules scan every non-binary text file, including comments, strings, hidden files (such as `.env` and `.config/`), and unknown extensions; `.git` is always excluded. AST/taint/metric rules only run on recognized languages.

### Metrics (built-in; do not redeclare in language files)

`metric/cyclomatic-complexity` (threshold 25), `metric/function-length` (200 lines), `metric/nesting-depth` (5), `metric/duplicate-block` (window of 12 lines, ≥2 occurrences), `metric/unreachable-code`. Thresholds are declared in `builtin.yaml` under `options`; custom rules may declare a metric rule of the same kind, and findings use that rule's id/severity/dimension/message. All file-level metrics and `metric/duplicate-block` support `options.skip_tests: 1`: test files (direct `tests/` files, `.test.`/`.spec.`, `test_*`) and Rust `mod tests` ranges are skipped; built-in metrics enable it by default.

## Rule packs (local MVP)

Packs install under `~/.codescan/packs/<id>/<version>/` and require `pack.yaml`:

```yaml
id: com.example.java-security
version: 1.0.0
engine_min: 1.5.0       # optional
engine_max: 2.0.0       # optional
description: Java security pack
rules:
  - rules.yaml
```

The desktop rules page and CLI `codescan pack` manage packs; pack rules are read-only and display `<id>@<version>` as their origin. Installation writes a content hash (`.integrity`) and loading verifies it, so tampering invalidates the pack. Scan history records enabled pack id/version/integrity/signed_by; `codescan scan` loads installed packs automatically. Install and scan validate compilation and id conflicts.

### Signatures and trust

Packs can carry a detached Ed25519 signature in `.signature`:

```json
{
  "algorithm": "ed25519",
  "key_id": "<sha256(public key) hex>",
  "signature": "<base64 64-byte signature>"
}
```

- The signed message is `codescan-pack-integrity:v1:<content hash>`; the hash covers `pack.yaml` and the manifest-declared rule files, not `.signature` / `.integrity`.
- Trusted public keys live at `<data_root>/trusted-keys/<key_id>.pub` as 32 raw bytes.
- With no trusted keys: unsigned packs are allowed; signed packs are rejected because they cannot be verified.
- With trusted keys configured: every pack must be signed by a trusted key; unsigned packs are rejected.
- Malformed signatures, untrusted keys, and mismatched signatures are rejected at install and scan time.
- CLI flow: `pack keygen --out keys` → `pack trust add keys/public.key` → `pack sign ./my-pack --key keys/private.key` → `pack install ./my-pack`.
- Private keys are only used for local signing and never enter packs, scan history, or list output. Remote downloads and a pack registry remain out of scope.

## Rule quality gates

- Every rule needs at least one positive fixture; negatives live in the same fixture directory and the exact-multiset runner rejects unannotated hits.
- Messages state structural constructs only — never exploitability; taint rules use "may reach" wording.
- Severity/dimension are rule-declared; changing an existing rule id's semantics requires a version advance.
- New rules must not add unannotated hits to existing fixtures; the full rule suite must pass before merge.

## Fixtures (rule unit tests)

Directory: `crates/engine/tests/fixtures/rules/<rule-id>/`; the file extension selects the language.

- The expected line is the **next non-empty, non-annotation line** pointed at by the annotation block: `// ruleid: <id>` (TS/TSX/JS/JSX/Java/Go/Rust) or `# ruleid: <id>` (Python). Stack several annotations when several rules hit the same line.
- The runner executes **all enabled rules** per fixture directory. The expected multiset and the actual hits (triples of rule id, file, start line) must match **in count**: an annotated line that did not match is a false negative; an unannotated hit (including hits from other rules or metrics) is an over-match. Both fail the test.
- Fixtures must therefore be *clean*: no unrelated rules or metrics may trigger (keep functions short, avoid repeated 12-line blocks, avoid content that collides with other text rules).

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
6. **Alias/dot/glob imports**: source/sink/sanitizer queries may match by function name or unconstrained package name and use an `imports` gate to scope the file; for example, both `s.Atoi` and `Atoi` can be sanitizers.
7. **Cross-function field writes and identity**: summaries keep `param.field ← param/internal taint` and parameter object identity sets (conditional/union returns prune statically known conditions (including local constant assignments, if/else/loop/switch/try branch merging (including switch fallthrough, labeled/unlabeled break/continue, and next-iteration labeled continue states; labels remain function-local), and statically known if conditions) and otherwise use the union of value branches), and replay them at call sites; `fill(o, v){ o.value = v }` and `id(o){ return o }` both keep `o.value` tainted in the caller. Full object heap graphs are not propagated.
6. When `@match` captures several nodes, the **smallest span** is used; sink matching requires both node kind and byte range equality.
7. Evidence: `flows` lists the source locations (multiple paths across files/functions are merged); the same sink yields one finding. SARIF `properties.flows` mirrors it.
8. Message wording: state only the approximation that "untrusted data may reach" the sink — never claim cross-file completeness or heap-sensitive proof.

## Inheritance rules (v2 type layer, Java)

```yaml
  - id: java/trust-all-certs-inherited
    language: java                # currently Java only
    dimension: security
    severity: warning
    message: Empty check methods inherited from X509TrustManager (needs confirmation)
    inheritance:
      target: X509TrustManager    # target interface/supertype (simple or qualified)
      methods:
        - checkClientTrusted
        - checkServerTrusted
      empty_only: true            # empty bodies only
      indirect_only: true         # skip direct implementations already covered by AST rules
```

Semantics and limits (Domain §4c):

1. Resolves `class_declaration`, `interface_declaration`, anonymous class bodies, `extends`/`implements`, class imports, and wildcard package imports within the workspace by package/import/qualified name; `indirect_only` reports classes that reach the target through inheritance but do not implement it directly.
2. Direct and direct-anonymous implementations stay covered by AST rules. Unresolved external supertypes, reflection/proxies, generics, and dynamic dispatch are not inferred — no proof, no finding.
3. Span = method declaration; evidence = method source text; fingerprint/degraded/order follow the common finding model.

## Ignore rules (scan scope)

The scan scope is determined by four ignore layers:

1. workspace `.gitignore` (effective even without a `.git` directory);
2. workspace-root `.codescanignore` (gitignore syntax);
3. application-global ignore file `~/.codescan/ignore` (gitignore syntax, applies to any workspace);
4. built-in default directories: `node_modules`, `target`, `dist`, `build`, `.venv`, `__pycache__`, `.next`, `coverage`, `.git`.

Hidden files and directories are scanned (for example `.env` and `.config/`); text rules also run on unknown extensions, while `.git` is always excluded. Ignored files are not reported individually; they are counted in the `IgnoredSummary` scan event.
