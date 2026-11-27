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
    pub captures: Vec<(String, String)>,
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
            for cap in m.captures() {
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
            let captures: Vec<(String, String)> = m
                .captures()
                .iter()
                .map(|cap| {
                    let name = names.get(cap.index as usize).cloned().unwrap_or_default();
                    let text = source
                        .get(cap.node.start_byte()..cap.node.end_byte())
                        .unwrap_or("")
                        .to_string();
                    (name, text)
                })
                .collect();
            out.push(Endpoint {
                start: sp,
                end: se,
                kind,
                span: span_of_bytes(sp, se, source),
                name,
                text,
                captures,
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
    let facts = facts::extract_file(rule, lang, tree, source, path, degraded, &[]);
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
        let parsed = crate::language::parse(lang, source.as_bytes()).expect("parse");
        extract_file(rule, lang, &parsed.tree, source, path, false, &[])
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
    fn cross_function_field_write_propagates_to_caller() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.value = v;\n}\nfunction f(input: string) {\n  const o: any = {};\n  fill(o, input);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 7), "{findings:?}");
    }

    #[test]
    fn cross_function_field_write_through_local_alias() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  const x = o;\n  x.value = v;\n}\nfunction f(input: string) {\n  const o: any = {};\n  fill(o, input);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 8), "{findings:?}");
    }

    #[test]
    fn cross_function_nested_field_write_propagates() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.inner.value = v;\n}\nfunction f(input: string) {\n  const o: any = { inner: {} };\n  fill(o, input);\n  eval(o.inner.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 7), "{findings:?}");
    }

    #[test]
    fn cross_function_returned_object_identity_propagates() {
        let rule = ts_param_rule();
        let source = "function id(o: any) {\n  return o;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b = id(a);\n  b.value = input;\n  eval(a.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 8), "{findings:?}");
    }

    #[test]
    fn cross_function_returned_field_identity_propagates() {
        let rule = ts_param_rule();
        let source = "function inner(o: any) {\n  return o.inner;\n}\nfunction f(input: string) {\n  const a: any = { inner: {} };\n  const b = inner(a);\n  b.value = input;\n  eval(a.inner.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 8), "{findings:?}");
    }

    #[test]
    fn cross_function_conditional_return_aliases_both_branches() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, flag: boolean) {\n  return flag ? a : b;\n}\nfunction f(input: string, flag: boolean) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, flag);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.span.start.line == 9 || f.span.start.line == 10)
                .count(),
            2,
            "{findings:?}"
        );
    }

    #[test]
    fn cross_function_nullish_return_aliases_both_branches() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  return a ?? b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.span.start.line == 9 || f.span.start.line == 10)
                .count(),
            2,
            "{findings:?}"
        );
    }

    #[test]
    fn cross_function_conditional_literal_branch_is_not_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, flag: boolean) {\n  return flag ? { ...a } : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.span.start.line == 9 || f.span.start.line == 10)
                .cloned()
                .collect::<Vec<_>>()
                .len(),
            1,
            "{findings:?}"
        );
        assert!(findings.iter().any(|f| f.span.start.line == 10), "{findings:?}");
    }

    #[test]
    fn cross_function_true_condition_prunes_alternative_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  return true ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 9, "{findings:?}");
    }

    #[test]
    fn cross_function_false_condition_prunes_consequence_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  return false ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 10, "{findings:?}");
    }

    #[test]
    fn cross_function_nullish_literal_selects_right_branch() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  return null ?? b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 10, "{findings:?}");
    }

    #[test]
    fn cross_function_const_condition_prunes_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  const flag = true;\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 10, "{findings:?}");
    }

    #[test]
    fn cross_function_constant_uses_value_at_expression_site() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  const c = flag ? a : b;\n  flag = false;\n  return c;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 12, "{findings:?}");
    }

    #[test]
    fn cross_function_reassigned_constant_prunes_latest_branch() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  flag = false;\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 12, "{findings:?}");
    }

    #[test]
    fn cross_function_const_nullish_prunes_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  const flag = null;\n  return flag ?? b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 11, "{findings:?}");
    }

    #[test]
    fn cross_function_if_else_same_constant_keeps_identity_branch() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag;\n  if (cond) {\n    flag = true;\n  } else {\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 15, "{findings:?}");
    }

    #[test]
    fn cross_function_if_else_different_constants_merges_to_unknown() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag;\n  if (cond) {\n    flag = true;\n  } else {\n    flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.span.start.line == 15 || f.span.start.line == 16)
                .count(),
            2,
            "{findings:?}"
        );
    }

    #[test]
    fn cross_function_if_without_else_merges_with_current_value() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  if (cond) {\n    flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.span.start.line == 13 || f.span.start.line == 14)
                .count(),
            2,
            "{findings:?}"
        );
    }

    #[test]
    fn cross_function_true_if_prunes_else_branch_constants() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  if (true) {\n    flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 14, "{findings:?}");
    }

    #[test]
    fn cross_function_false_if_selects_else_branch_constants() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag;\n  if (false) {\n    flag = false;\n  } else {\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 15, "{findings:?}");
    }

    #[test]
    fn cross_function_while_may_skip_body_merges_to_unknown() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  while (cond) {\n    flag = false;\n    break;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, false);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_while_same_constant_keeps_branch() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  while (cond) {\n    flag = true;\n    break;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 14, "{findings:?}");
    }

    #[test]
    fn cross_function_do_while_applies_body_constant() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  do {\n    flag = false;\n    break;\n  } while (cond);\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, false);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 15, "{findings:?}");
    }

    #[test]
    fn cross_function_switch_different_cases_merge_to_unknown() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, n: number) {\n  let flag;\n  switch (n) {\n    case 1: flag = true; break;\n    default: flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, 1);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_switch_same_cases_keep_branch() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, n: number) {\n  let flag;\n  switch (n) {\n    case 1: flag = true; break;\n    default: flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, 1);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 14, "{findings:?}");
    }

    #[test]
    fn cross_function_try_catch_different_constants_merge_to_unknown() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  try {\n    flag = false;\n  } catch {\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_try_finally_applies_constant() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = false;\n  try {\n    flag = true;\n  } finally {\n    flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 16, "{findings:?}");
    }

    #[test]
    fn cross_function_switch_fallthrough_until_break() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, n: number) {\n  let flag;\n  switch (n) {\n    case 1: flag = true;\n    case 2: flag = false; break;\n    default: flag = false;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, 1);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 16, "{findings:?}");
    }

    #[test]
    fn cross_function_do_while_break_is_path_sensitive() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  do {\n    flag = false;\n    if (cond) break;\n    flag = true;\n  } while (false);\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_do_while_continue_is_path_sensitive() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  do {\n    flag = false;\n    if (cond) continue;\n    flag = true;\n  } while (false);\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_labeled_break_exits_outer_loop() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  outer: for (let i = 0; i < 1; i++) {\n    while (cond) {\n      flag = false;\n      break outer;\n    }\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_labeled_continue_targets_outer_loop() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  let flag = true;\n  outer: for (let i = 0; i < 1; i++) {\n    while (cond) {\n      flag = false;\n      continue outer;\n    }\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, true);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn cross_function_labeled_block_break_skips_rest() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  block: {\n    flag = false;\n    break block;\n    flag = true;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 16, "{findings:?}");
    }

    #[test]
    fn cross_function_labeled_continue_applies_next_iteration_state() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  let flag = true;\n  outer: for (;;) {\n    if (flag) {\n      flag = false;\n      continue outer;\n    }\n    flag = true;\n    break outer;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 18, "{findings:?}");
    }

    #[test]
    fn labels_do_not_propagate_into_or_out_of_nested_functions() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any): any {\n  let flag = true;\n  outer: for (let i = 0; i < 1; i++) {\n    function inner() {\n      label: {\n        flag = false;\n        break label;\n      }\n    }\n    flag = true;\n    break outer;\n  }\n  return flag ? a : b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 20, "{findings:?}");
    }

    #[test]
    fn cross_function_conditional_field_write_respects_false_argument() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, cond: boolean) {\n  if (cond) {\n    o.value = v;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, false);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn cross_function_conditional_field_write_applies_with_true_argument() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, cond: boolean) {\n  if (cond) {\n    o.value = v;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, true);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 9, "{findings:?}");
    }

    #[test]
    fn cross_function_unreachable_return_is_not_an_identity() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any) {\n  if (false) {\n    return a;\n  }\n  return b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 13, "{findings:?}");
    }

    #[test]
    fn cross_function_conditional_return_respects_false_argument() {
        let rule = ts_param_rule();
        let source = "function choose(a: any, b: any, cond: boolean) {\n  if (cond) {\n    return a;\n  }\n  return b;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b: any = {};\n  const c = choose(a, b, false);\n  c.value = input;\n  eval(a.value);\n  eval(b.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 13, "{findings:?}");
    }

    #[test]
    fn cross_function_loop_guard_controls_field_effect() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, cond: boolean) {\n  while (cond) {\n    o.value = v;\n    break;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, false);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn cross_function_loop_guard_allows_true_field_effect() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, cond: boolean) {\n  while (cond) {\n    o.value = v;\n    break;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, true);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn cross_function_unreachable_call_does_not_apply_effect() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.value = v;\n}\nfunction f(input: string) {\n  const o: any = {};\n  if (false) {\n    fill(o, input);\n  }\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn cross_function_switch_matched_case_applies_effect() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.value = v;\n}\nfunction f(input: string, mode: number) {\n  const o: any = {};\n  switch (mode) {\n    case 1: fill(o, input); break;\n  }\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn unreachable_sink_is_not_reported() {
        let rule = ts_param_rule();
        let source = "function f(input: string) {\n  if (false) {\n    eval(input);\n  }\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn unreachable_assignment_is_not_applied() {
        let rule = ts_param_rule();
        let source = "function f(input: string) {\n  const o: any = {};\n  if (false) {\n    o.value = input;\n  }\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn compound_guard_blocks_false_effect() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, cond: boolean, other: boolean) {\n  if (cond && other) {\n    o.value = v;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, false, false);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn switch_case_guard_blocks_unmatched_effect() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, n: number) {\n  switch (n) {\n    case 1: o.value = v; break;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, 2);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn switch_case_guard_allows_matching_effect() {
        let rule = ts_param_rule();
        let source = "function maybe(o: any, v: string, n: number) {\n  switch (n) {\n    case 1: o.value = v; break;\n  }\n}\nfunction f(input: string) {\n  const o: any = {};\n  maybe(o, input, 1);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn cross_function_clone_return_does_not_alias() {
        let rule = ts_param_rule();
        let source = "function clone(o: any) {\n  return { ...o };\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b = clone(a);\n  b.value = input;\n  eval(a.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn cross_function_identity_plus_field_write_propagates() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.value = v;\n}\nfunction id(o: any) {\n  return o;\n}\nfunction f(input: string) {\n  const a: any = {};\n  const b = id(a);\n  fill(b, input);\n  eval(a.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.iter().any(|f| f.span.start.line == 11), "{findings:?}");
    }

    #[test]
    fn cross_function_literal_field_write_stays_clean() {
        let rule = ts_param_rule();
        let source = "function fill(o: any, v: string) {\n  o.value = \"safe\";\n}\nfunction f(input: string) {\n  const o: any = {};\n  fill(o, input);\n  eval(o.value);\n}\n";
        let findings = analyze(&rule, &[("f.ts", Language::TypeScript, source)]);
        assert!(findings.is_empty(), "{findings:?}");
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

    // ---- v6：points-to 集合 + 形状敏感 ----

    #[test]
    fn p1_nested_shape_reads() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(): void {\n  const box = { a: { b: read() } };\n  eval(box.a.b);\n  eval(box.a.c);\n  eval(box.b);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![3], "仅 box.a.b 命中：{findings:?}");
    }

    #[test]
    fn p2_nested_writes() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(obj: any): void {\n  obj.a.b = read();\n  eval(obj.a.b);\n  eval(obj.a.c);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![3], "仅 obj.a.b 命中：{findings:?}");
    }

    #[test]
    fn p3_field_pointer_alias() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(): void {\n  const base: any = {};\n  const box = { x: base };\n  box.x.f = read();\n  eval(base.f);\n  eval(box.y);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![5], "字段指针别名应经 base.f 可见：{findings:?}");
    }

    #[test]
    fn p4_set_merge_and_strong_update() {
        let rule = ts_rule(TS_READ_SOURCE);
        let merged = "function f(): void {\n  const a: any = {};\n  const b: any = {};\n  let c: any = a;\n  c.f = read();\n  eval(a.f);\n  eval(b.f);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, merged)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![6], "别名集合写入仅命中 a.f：{findings:?}");

        let reassigned = "function g(): void {\n  const a: any = {};\n  let c: any = a;\n  c = {};\n  c.f = read();\n  eval(a.f);\n  eval(c.f);\n}\n";
        let findings = analyze(&rule, &[("t.ts", Language::TypeScript, reassigned)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![7], "重赋值后旧对象不命中：{findings:?}");
    }

    #[test]
    fn p5_wildcard_index() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(obj: any, key: string): void {\n  obj[key] = read();\n  eval(obj.any);\n  eval(obj[key]);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![3, 4], "通配写后任意读命中：{findings:?}");

        let known = "function g(obj: any, key: string): void {\n  obj.known = read();\n  eval(obj[key]);\n}\n";
        let findings = analyze(&rule, &[("t.ts", Language::TypeScript, known)]);
        assert!(findings.iter().any(|f| f.span.start.line == 3), "{findings:?}");
    }

    #[test]
    fn p7_array_elements() {
        let rule = ts_rule(TS_READ_SOURCE);
        let source = "function f(): void {\n  const arr = [read()];\n  eval(arr[0]);\n  eval(arr);\n}\n";
        let findings = analyze(&rule, &[("s.ts", Language::TypeScript, source)]);
        let lines: Vec<usize> = findings.iter().map(|f| f.span.start.line).collect();
        assert_eq!(lines, vec![3, 4], "数组元素与整体读取命中：{findings:?}");
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

