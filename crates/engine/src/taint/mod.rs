//! 污点分析（Domain §4a/4b）：
//! - 事实提取 IR（每文件，规则相关端点 + 结构语句/表达式）；
//! - 项目级分析：显式导入跨文件（TS/JS、Python）、Go 同包；
//! - 字段敏感访问路径与对象组别名近似；
//! - 布尔摘要不动点（支持递归）。

mod engine;
mod facts;

pub use engine::analyze_project;
pub use facts::{extract_file, AccessPath, CallFact, ExprFact, FileFacts, FnFact, ImportFact, SinkFact, StmtFact};

use crate::compile::TaintRule;
use crate::language::Language;
use crate::model::{Finding, Position, Span};
use tree_sitter::{Node, StreamingIterator as _};

/// 端点（source/sink/sanitizer 命中）。
#[derive(Debug, Clone)]
pub(crate) struct Endpoint {
    pub start: usize,
    pub end: usize,
    /// @match 节点种类（sink 判定要求种类与字节范围双等值）。
    pub kind: String,
    pub span: Span,
    pub name: Option<String>,
    pub text: String,
}

pub(crate) fn collect_endpoints(
    queries: &[tree_sitter::Query],
    root: Node<'_>,
    source: &str,
) -> Vec<Endpoint> {
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut out = Vec::new();
    for query in queries {
        let names: Vec<String> = query
            .capture_names()
            .iter()
            .map(|n| n.to_string())
            .collect();
        let mut it = cursor.matches(query, root, source.as_bytes());
        while let Some(m) = it.next() {
            let mut union = Option::<(usize, usize)>::None;
            let mut best_match = Option::<(usize, usize, &str)>::None;
            for cap in m.captures {
                let name = names
                    .get(cap.index as usize)
                    .cloned()
                    .unwrap_or_default();
                let candidate = (cap.node.start_byte(), cap.node.end_byte());
                union = Some(match union {
                    Some((s0, e0)) => (s0.min(candidate.0), e0.max(candidate.1)),
                    None => candidate,
                });
                if name == "match" {
                    best_match = Some(match best_match {
                        Some((s0, e0, k))
                            if (e0 - s0, s0) <= (candidate.1 - candidate.0, candidate.0) =>
                        {
                            (s0, e0, k)
                        }
                        _ => (candidate.0, candidate.1, cap.node.kind()),
                    });
                }
            }
            let Some(union_range) = union else {
                continue;
            };
            let (sp, se, kind) = match best_match {
                Some((s0, e0, kind)) => (s0, e0, kind.to_string()),
                None => (
                    union_range.0,
                    union_range.1.min(source.len()),
                    "<union>".to_string(),
                ),
            };
            let text = source.get(sp..se).unwrap_or("").to_string();
            let identifier_like = kind.contains("identifier") || kind == "attribute";
            let name = identifier_like.then(|| text.clone());
            out.push(Endpoint {
                start: sp,
                end: se,
                kind,
                span: span_of_bytes(sp, se, source),
                name,
                text,
            });
        }
    }
    out.sort_by_key(|e| (e.start, e.end));
    out
}

/// 同文件分析入口（单测/兼容层）：提取单文件事实后走项目分析。
pub fn taint_findings(
    rule: &TaintRule,
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> Vec<Finding> {
    let facts = facts::extract_file(rule, lang, tree, source, path, degraded);
    analyze_project(rule, &[facts])
}

pub(crate) fn span_of_bytes(start: usize, end: usize, source: &str) -> Span {
    let locate = |byte: usize| -> Position {
        let mut line = 1usize;
        let mut col = 1usize;
        for (idx, ch) in source.char_indices() {
            if idx >= byte {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        Position { line, column: col }
    };
    Span {
        start: locate(start),
        end: locate(end),
    }
}

pub(crate) fn named_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'tree>> = node.named_children(&mut cursor).collect();
    drop(cursor);
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile, CompiledRule};
    use crate::rules::parse_rule_set;

    fn build_rule(yaml: &str) -> TaintRule {
        let specs = parse_rule_set(yaml).expect("parse");
        let compiled = compile(specs).expect("compile");
        match compiled.into_iter().next().unwrap() {
            CompiledRule::Taint(r) => r,
            _ => panic!("not taint"),
        }
    }

    fn python_rule(source_pattern: &str) -> TaintRule {
        let yaml = format!(
            "rules:\n  - id: probe/taint\n    language: python\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          {}\n      sinks:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n      sanitizers:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"int\")) @match\n",
            source_pattern
        );
        build_rule(&yaml)
    }

    fn ts_param_rule() -> TaintRule {
        ts_rule("(function_declaration\n            parameters: (formal_parameters\n              (required_parameter (identifier) @match))) @match")
    }

    fn ts_rule(source_pattern: &str) -> TaintRule {
        let yaml = format!(
            "rules:\n  - id: probe/ts\n    language: typescript\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          {}\n      sinks:\n        - |\n          (call_expression\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n",
            source_pattern
        );
        build_rule(&yaml)
    }

    fn java_rule() -> TaintRule {
        let yaml = "rules:\n  - id: probe/java\n    language: java\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          (method_declaration\n            parameters: (formal_parameters\n              (formal_parameter name: (identifier) @match))) @match\n      sinks:\n        - |\n          (method_invocation\n            name: (identifier) @fn (#eq? @fn \"eval\")) @match\n";
        build_rule(yaml)
    }

    fn rust_rule() -> TaintRule {
        let yaml = "rules:\n  - id: probe/rust\n    language: rust\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          (function_item\n            parameters: (parameters\n              (parameter pattern: (identifier) @match))) @match\n      sinks:\n        - |\n          (call_expression\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n";
        build_rule(yaml)
    }

    fn go_rule() -> TaintRule {
        let yaml = "rules:\n  - id: probe/go\n    language: go\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          (function_declaration\n            parameters: (parameter_list\n              (parameter_declaration name: (identifier) @match))) @match\n      sinks:\n        - |\n          (call_expression\n            function: (identifier) @fn (#eq? @fn \"sink\")) @match\n";
        build_rule(yaml)
    }

    fn facts_for(rule: &TaintRule, lang: Language, path: &str, source: &str) -> FileFacts {
        let parsed = crate::language::parse(lang, source.as_bytes());
        extract_file(rule, lang, &parsed.tree, source, path, false)
    }

    fn analyze(rule: &TaintRule, files: &[(&str, Language, &str)]) -> Vec<Finding> {
        let facts: Vec<FileFacts> = files
            .iter()
            .map(|(path, lang, source)| facts_for(rule, *lang, path, source))
            .collect();
        let mut findings = analyze_project(rule, &facts);
        findings.sort_by_key(|f| (f.path.clone(), f.span.start.line, f.span.start.column));
        findings
    }

    fn scan_py(source_pattern: &str, source: &str) -> Vec<Finding> {
        let rule = python_rule(source_pattern);
        analyze(&rule, &[("s.py", Language::Python, source)])
    }

    const INPUT_SOURCE: &str = "(call\n            function: (identifier) @fn (#eq? @fn \"input\")) @match";
    const PARAM_SOURCE: &str = "(function_definition\n            parameters: (parameters (identifier) @match)) @match";

    // ---- v2/v4 兼容：同文件语义 ----

    #[test]
    fn reachable_flow_reports_with_evidence() {
        let findings = scan_py(INPUT_SOURCE, "def f():\n    eval(input(\"x\"))\n");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 2);
        assert!(!findings[0].flows.is_empty());
    }

    #[test]
    fn assignment_chain_propagates() {
        let findings = scan_py(INPUT_SOURCE, "def f():\n    v = input(\"x\")\n    eval(v)\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].span.start.line, 3);
    }

    #[test]
    fn sanitizer_cuts_flow() {
        assert!(scan_py(INPUT_SOURCE, "def f():\n    v = int(input(\"x\"))\n    eval(v)\n")
            .is_empty());
    }

    #[test]
    fn no_chain_and_cross_function_param_isolation() {
        assert!(scan_py(INPUT_SOURCE, "def f():\n    eval(\"1\")\n").is_empty());
        assert!(scan_py(INPUT_SOURCE, "def a():\n    v = input()\n\ndef b():\n    eval(v)\n")
            .is_empty());
    }

    #[test]
    fn d1_call_chain_reaches_sink_with_caller_flow() {
        let source = "def handle(user_input):\n    run(user_input)\n\ndef run(code):\n    eval(code)\n";
        let findings = scan_py(PARAM_SOURCE, source);
        let sink: Vec<&Finding> = findings.iter().filter(|f| f.span.start.line == 5).collect();
        assert_eq!(sink.len(), 1, "sink 应恰一条：{findings:?}");
        assert!(sink[0].flows.iter().any(|p| p.line == 1));
    }

    #[test]
    fn d2_return_value_flow() {
        let source = "def helper():\n    return input()\n\ndef f():\n    v = helper()\n    eval(v)\n";
        let findings = scan_py(INPUT_SOURCE, source);
        assert!(findings.iter().any(|f| f.span.start.line == 6), "{findings:?}");
    }

    #[test]
    fn d3_sanitizer_at_call_boundary() {
        let source = "def run(code):\n    eval(code)\n\ndef safe(v):\n    run(int(v))\n";
        let findings = scan_py(PARAM_SOURCE, source);
        let sink = findings
            .iter()
            .find(|f| f.span.start.line == 2)
            .expect("run 自身参数作为 source 命中");
        assert!(!sink.flows.iter().any(|p| p.line == 5), "{:?}", sink.flows);
    }

    #[test]
    fn d4_recursion_terminates_and_is_deterministic() {
        let source = "def a(x):\n    b(x)\n\ndef b(y):\n    eval(y)\n    a(y)\n";
        let first = scan_py(PARAM_SOURCE, source);
        let second = scan_py(PARAM_SOURCE, source);
        assert!(!first.is_empty());
        let key = |v: &[Finding]| {
            format!(
                "{:?}",
                v.iter()
                    .map(|f| (f.span.start.line, f.flows.clone()))
                    .collect::<Vec<_>>()
            )
        };
        assert_eq!(key(&first), key(&second));
    }

    #[test]
    fn d6_same_name_union() {
        let source = "def helper(x):\n    eval(x)\n\ndef helper(y):\n    print(y)\n\ndef entry(v):\n    helper(v)\n";
        let findings = scan_py(PARAM_SOURCE, source);
        assert!(findings.iter().any(|f| f.span.start.line == 2), "{findings:?}");
    }

    // ---- v5：跨文件 ----

    #[test]
    fn f1_ts_cross_file_import() {
        let rule = ts_param_rule();
        let a = "import { run } from \"./b\";\n\nexport function entry(userInput: string): void {\n  run(userInput);\n}\n";
        let b = "export function run(code: string): void {\n  eval(code);\n}\n";
        let findings = analyze(&rule, &[("a.ts", Language::TypeScript, a), ("b.ts", Language::TypeScript, b)]);
        let sink: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.path == "b.ts" && f.span.start.line == 2)
            .collect();
        assert_eq!(sink.len(), 1, "{findings:?}");
        assert!(
            sink[0].flows.iter().any(|p| p.line == 3),
            "flows 应含 a.ts 调用者参数源位置：{:?}",
            sink[0].flows
        );
    }

    #[test]
    fn f1_python_cross_file_import() {
        let rule = python_rule(PARAM_SOURCE);
        let a = "from b import run\n\ndef entry(user_input):\n    run(user_input)\n";
        let b = "def run(code):\n    eval(code)\n";
        let findings = analyze(&rule, &[("a.py", Language::Python, a), ("b.py", Language::Python, b)]);
        let sink = findings
            .iter()
            .find(|f| f.path == "b.py" && f.span.start.line == 2)
            .expect("b.py sink 命中");
        assert!(sink.flows.iter().any(|p| p.line == 3), "{:?}", sink.flows);
    }

    #[test]
    fn f2_go_same_package_cross_file() {
        let rule = go_rule();
        let a = "package pkg\n\nfunc entry(userInput string) {\n\trun(userInput)\n}\n";
        let b = "package pkg\n\nfunc run(code string) {\n\tsink(code)\n}\n";
        let findings = analyze(
            &rule,
            &[("pkg/a.go", Language::Go, a), ("pkg/b.go", Language::Go, b)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "pkg/b.go" && f.span.start.line == 4)
            .expect("Go 同包 sink 命中");
        assert!(sink.flows.iter().any(|p| p.line == 3), "{:?}", sink.flows);
    }

    #[test]
    fn f3_unimported_same_name_does_not_cross_files() {
        let rule = python_rule(PARAM_SOURCE);
        let a = "\n\ndef entry(user_input):\n    run(user_input)\n";
        let b = "def run(code):\n    eval(code)\n";
        let findings = analyze(&rule, &[("a.py", Language::Python, a), ("b.py", Language::Python, b)]);
        let sink = findings
            .iter()
            .find(|f| f.path == "b.py" && f.span.start.line == 2)
            .expect("b.py 自身参数命中");
        assert!(
            !sink.flows.iter().any(|p| p.line >= 3),
            "无导入不应引入 a.py 流：{:?}",
            sink.flows
        );
    }

    // ---- v5.2：Java / Rust 跨文件 ----

    #[test]
    fn h1_java_class_import_qualified_call() {
        let rule = java_rule();
        let utils = "package a;\n\nclass Utils {\n  static void run(String code) {\n    eval(code);\n  }\n}\n";
        let entry = "package b;\n\nimport a.Utils;\n\nclass Entry {\n  static void entry(String userInput) {\n    Utils.run(userInput);\n  }\n}\n";
        let findings = analyze(
            &rule,
            &[("a/Utils.java", Language::Java, utils), ("b/Entry.java", Language::Java, entry)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "a/Utils.java" && f.span.start.line == 5)
            .expect("Java 类导入跨文件命中");
        assert!(sink.flows.iter().any(|p| p.line == 6), "{:?}", sink.flows);
    }

    #[test]
    fn h2_java_static_import_bare_call() {
        let rule = java_rule();
        let utils = "package a;\n\nclass Utils {\n  static void run(String code) {\n    eval(code);\n  }\n}\n";
        let entry = "package b;\n\nimport static a.Utils.run;\n\nclass Entry {\n  static void entry(String userInput) {\n    run(userInput);\n  }\n}\n";
        let findings = analyze(
            &rule,
            &[("a/Utils.java", Language::Java, utils), ("b/Entry.java", Language::Java, entry)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "a/Utils.java" && f.span.start.line == 5)
            .expect("静态导入跨文件命中");
        assert!(sink.flows.iter().any(|p| p.line == 6), "{:?}", sink.flows);
    }

    #[test]
    fn h1_negative_java_without_import() {
        let rule = java_rule();
        let utils = "package a;\n\nclass Utils {\n  static void run(String code) {\n    eval(code);\n  }\n}\n";
        let entry = "package b;\n\nclass Entry {\n\n  // 无 import\n\n  static void entry(String userInput) {\n    Utils.run(userInput);\n  }\n}\n";
        let findings = analyze(
            &rule,
            &[("a/Utils.java", Language::Java, utils), ("b/Entry.java", Language::Java, entry)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "a/Utils.java" && f.span.start.line == 5)
            .expect("Utils 自身参数命中");
        assert!(
            !sink.flows.iter().any(|p| p.line == 7),
            "无 import 不应引入 Entry 流：{:?}",
            sink.flows
        );
    }

    #[test]
    fn h3_rust_use_import_and_alias() {
        let rule = rust_rule();
        let util = "pub fn run(code: &str) {\n    eval(code);\n}\n";
        let lib = "use crate::util::run as r;\n\npub fn entry(v: &str) {\n    r(v);\n}\n";
        let findings = analyze(
            &rule,
            &[("src/lib.rs", Language::Rust, lib), ("src/util.rs", Language::Rust, util)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "src/util.rs" && f.span.start.line == 2)
            .expect("Rust use 别名跨文件命中");
        assert!(sink.flows.iter().any(|p| p.line == 3), "{:?}", sink.flows);
    }

    #[test]
    fn h4_rust_qualified_call() {
        let rule = rust_rule();
        let util = "pub fn run(code: &str) {\n    eval(code);\n}\n";
        let lib = "pub fn entry(v: &str) {\n    crate::util::run(v);\n}\n";
        let findings = analyze(
            &rule,
            &[("src/lib.rs", Language::Rust, lib), ("src/util.rs", Language::Rust, util)],
        );
        let sink = findings
            .iter()
            .find(|f| f.path == "src/util.rs" && f.span.start.line == 2)
            .expect("Rust 限定调用命中");
        assert!(sink.flows.iter().any(|p| p.line == 1), "{:?}", sink.flows);
    }

    // ---- v5：字段敏感与堆别名 ----

    const TS_READ_SOURCE: &str = "(call_expression\n            function: (identifier) @fn (#eq? @fn \"read\")) @match";

    #[test]
    fn f4_field_sensitive_writes_and_literals() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(obj: any): void {\n  const v = read();\n  obj.a = v;\n  eval(obj.b);\n  eval(obj.a);\n  const box = { a: read(), b: \"x\" };\n  eval(box.b);\n  eval(box.a);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert!(lines.contains(&5), "obj.a 应命中：{lines:?}");
        assert!(lines.contains(&8), "box.a 应命中：{lines:?}");
        assert!(!lines.contains(&4), "obj.b 不应命中：{lines:?}");
        assert!(!lines.contains(&7), "box.b 不应命中：{lines:?}");
    }

    #[test]
    fn f5_heap_alias_and_strong_update() {
        let rule = ts_rule(TS_READ_SOURCE);
        let alias_ok = "function f(obj: any): void {\n  const alias = obj;\n  alias.a = read();\n  eval(obj.a);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, alias_ok)]);
        assert!(
            findings.iter().any(|f| f.span.start.line == 4),
            "别名写入应经 obj.a 可见：{findings:?}"
        );

        let alias_reassigned = "function f(obj: any): void {\n  let alias = obj;\n  alias = {};\n  alias.a = read();\n  eval(obj.a);\n}\n";
        let findings = analyze(&rule, &[("t.ts", Language::TypeScript, alias_reassigned)]);
        assert!(
            findings.is_empty(),
            "重赋值后旧对象不应看到新写入：{findings:?}"
        );
    }

    #[test]
    fn f7_determinism_and_sanitizer_still_apply() {
        let rule = python_rule(INPUT_SOURCE);
        let source = "def f():\n    v = input()\n    w = int(v)\n    eval(w)\n    eval(v)\n";
        let first = analyze(&rule, &[("s.py", Language::Python, source)]);
        let second = analyze(&rule, &[("s.py", Language::Python, source)]);
        assert_eq!(
            format!("{:?}", first.iter().map(|f| (f.span.start.line, &f.flows)).collect::<Vec<_>>()),
            format!("{:?}", second.iter().map(|f| (f.span.start.line, &f.flows)).collect::<Vec<_>>()),
        );
        let lines: Vec<usize> = first.iter().map(|f| f.span.start.line).collect();
        assert!(lines.contains(&5), "eval(v) 应命中");
        assert!(!lines.contains(&4), "int() 清洗后 eval(w) 不应命中");
    }
}

