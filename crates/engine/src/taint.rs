//! 过程内污点分析（Domain §4a）：单函数体、变量名扁平化、顺序不敏感。
//! 发现 span = sink 命中；flows = sink 对应的 source 位置列表。

use crate::compile::TaintRule;
use crate::language::Language;
use crate::metrics;
use crate::model::{Dimension, Finding, Position, Severity, Span};
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use tree_sitter::StreamingIterator as _;

#[derive(Debug, Clone)]
struct Endpoint {
    start: usize,
    end: usize,
    /// @match 节点种类：sink 判定要求种类与字节范围双等值
    /// （python 里 block / expression_statement / call 可共享同一字节范围）。
    kind: String,
    span: Span,
    /// `@match` 捕获为标识符形态时的扁平变量名；调用/表达式形态为 None。
    name: Option<String>,
    /// 端点命中文本。
    text: String,
}

pub fn taint_findings(
    rule: &TaintRule,
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> Vec<Finding> {
    let root = tree.root_node();
    let prof = metrics::profile(lang);
    let mut cursor = tree_sitter::QueryCursor::new();

    let mut collect = |queries: &[tree_sitter::Query]| -> Vec<Endpoint> {
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
                    // 多个 @match 捕获时取最小「跨度」节点（如「函数声明 + 参数」
                    // 双 @match 端点，端点应是参数标识符而非整个函数；捕获顺序
                    // 不保证，以长度最小、起点更小者为准，确定且与写法顺序无关）。
                    if name == "match" {
                        best_match = Some(match best_match {
                            Some((s0, e0, k)) if (e0 - s0, s0) <= (candidate.1 - candidate.0, candidate.0) => {
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
        // 确定性：按位置排序。
        out.sort_by_key(|e| (e.start, e.end));
        out
    };

    let sources = collect(&rule.source_queries);
    let sinks = collect(&rule.sink_queries);
    let sanitizer_ranges: Vec<(usize, usize)> = collect(&rule.sanitizer_queries)
        .into_iter()
        .map(|e| (e.start, e.end))
        .collect();
    
    let mut findings = Vec::new();
    let mut functions = Vec::new();
    crate::metrics::walk_all_pub(root, &mut |node| {
        if prof.functions.contains(&node.kind()) {
            functions.push(node);
        }
    });

    for func in functions {
        let (fs, fe) = (func.start_byte(), func.end_byte());
        let fn_sinks: Vec<&Endpoint> = sinks
            .iter()
            .filter(|s| s.start >= fs && s.end <= fe)
            .collect();
        if fn_sinks.is_empty() {
            continue;
        }
        let fn_sources: Vec<&Endpoint> = sources
            .iter()
            .filter(|e| e.start >= fs && e.end <= fe)
            .collect();

        let mut tainted: HashSet<String> = HashSet::new();
        let mut flows: HashMap<String, Position> = HashMap::new();
        let mut source_ranges: Vec<(usize, usize, Position)> = Vec::new();
        for e in &fn_sources {
            source_ranges.push((e.start, e.end, e.span.start));
            if let Some(name) = &e.name {
                tainted.insert(name.clone());
                flows.entry(name.clone()).or_insert(e.span.start);
            }
        }
        let mut visit = |node: Node<'_>| {
            let (ns, ne) = (node.start_byte(), node.end_byte());

            // 赋值/声明传播。
            let assign = prof
                .assignments
                .iter()
                .find(|(kind, _, _)| *kind == node.kind())
                .copied();
            if let Some((_, lhs_field, rhs_field)) = assign {
                if let (Some(lhs), Some(rhs)) = (
                    node.child_by_field_name(lhs_field),
                    node.child_by_field_name(rhs_field),
                ) {
                    let (rs, re) = (rhs.start_byte(), rhs.end_byte());
                    let sanitized = sanitizer_ranges.iter().any(|(ss, se)| rs < *se && *ss < re);
                    if !sanitized {
                        let touches_source = source_ranges
                            .iter()
                            .any(|(s0, s1, _)| rs < *s1 && *s0 < re);
                        let tainted_idents: Vec<String> = identifiers_within(rhs, source)
                            .into_iter()
                            .filter(|(name, _)| tainted.contains(name))
                            .map(|(name, _)| name)
                            .collect();
                        if touches_source || !tainted_idents.is_empty() {
                            let flow_from = if let Some((_, _, pos)) = source_ranges
                                .iter()
                                .find(|(s0, s1, _)| rs < *s1 && *s0 < re)
                            {
                                Some(*pos)
                            } else {
                                tainted_idents
                                    .iter()
                                    .filter_map(|name| flows.get(name))
                                    .copied()
                                    .next()
                            };
                            for (name, _) in identifiers_within(lhs, source) {
                                tainted.insert(name.clone());
                                if let Some(pos) = flow_from {
                                    flows.entry(name).or_insert(pos);
                                }
                            }
                        }
                    }
                    // 不提前返回：赋值节点本身也可能是 sink（如 innerHTML 赋值）。
                }
            }

            // sink 命中：字节范围与端点完全相等。
            let Some(sink) = fn_sinks
                .iter()
                .find(|s| s.start == ns && s.end == ne && s.kind == node.kind())
            else {
                return;
            };

            // sanitizer 覆盖 sink → 清洗。
            if sanitizer_ranges
                .iter()
                .any(|(ss, se)| *ss <= sink.start && *se >= sink.end)
            {
                return;
            }
            let mut flow_positions: Vec<Position> = Vec::new();
            for (s0, s1, pos) in &source_ranges {
                // 直接流：sink 包含 source，且 source 未被 sanitizer 覆盖。
                let sanitized = sanitizer_ranges
                    .iter()
                    .any(|(ss, se)| *s0 >= *ss && *s1 <= *se);
                if *s0 >= sink.start && *s1 <= sink.end && !sanitized {
                    flow_positions.push(*pos);
                }
            }
            for ident_node in identifiers_within_nodes(node) {
                let (name, pos) = {
                    let text = source
                        .get(ident_node.start_byte()..ident_node.end_byte())
                        .unwrap_or("")
                        .to_string();
                    let pos = Position {
                        line: ident_node.start_position().row + 1,
                        column: ident_node.start_position().column + 1,
                    };
                    (text, pos)
                };
                // 落入 sanitizer 范围的标识符视为已清洗。
                let (is_, ie_) = (ident_node.start_byte(), ident_node.end_byte());
                let sanitized = sanitizer_ranges
                    .iter()
                    .any(|(ss, se)| is_ >= *ss && ie_ <= *se);
                if sanitized {
                    continue;
                }
                if tainted.contains(&name) {
                    if let Some(p) = flows.get(&name) {
                        flow_positions.push(*p);
                    } else {
                        flow_positions.push(pos);
                    }
                }
            }
            flow_positions.sort_by_key(|p| (p.line, p.column));
            flow_positions.dedup();
            
            if flow_positions.is_empty() {
                return;
            }
            findings.push(Finding {
                rule_id: rule.spec.id.clone(),
                dimension: rule.spec.dimension,
                severity: rule.spec.severity,
                path: path.to_string(),
                span: sink.span,
                message: rule.spec.message.clone(),
                match_text: sink.text.clone(),
                captures: Vec::new(),
                degraded,
                flows: flow_positions,
                fingerprint: String::new(),
            });
        };
        metrics::walk_all_pub(func, &mut visit);
    }
    findings
}

fn span_of_bytes(start: usize, end: usize, source: &str) -> Span {
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

/// 子树内标识符形态节点（保留节点供 sanitizer 范围判定）。
fn identifiers_within_nodes<'tree>(node: tree_sitter::Node<'tree>) -> Vec<tree_sitter::Node<'tree>> {
    let mut out = Vec::new();
    metrics::walk_all_pub(node, &mut |n| {
        if n.kind().contains("identifier") || n.kind() == "attribute" {
            out.push(n);
        }
    });
    out
}

/// 子树内标识符形态节点 → (扁平名, 位置)。
fn identifiers_within(node: Node<'_>, source: &str) -> Vec<(String, Position)> {
    let mut out = Vec::new();
    metrics::walk_all_pub(node, &mut |n| {
        if n.kind().contains("identifier") || n.kind() == "attribute" {
            let text = source
                .get(n.start_byte()..n.end_byte())
                .unwrap_or("")
                .to_string();
            if !text.is_empty() {
                out.push((
                    text,
                    Position {
                        line: n.start_position().row + 1,
                        column: n.start_position().column + 1,
                    },
                ));
            }
        }
    });
    out
}

// Dimension/Severity 供 finding() 内联构造使用，避免未使用导入。
const _: fn() = || {
    let _ = (Dimension::Security, Severity::Warning);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile, CompiledRule};
    use crate::rules::parse_rule_set;

    fn python_rule() -> TaintRule {
        let yaml = "rules:\n  - id: probe/taint\n    language: python\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"input\")) @match\n      sinks:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n      sanitizers:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"int\")) @match\n";
        let specs = parse_rule_set(yaml).expect("parse");
        let compiled = compile(specs).expect("compile");
        match compiled.into_iter().next().unwrap() {
            CompiledRule::Taint(r) => r,
            _ => panic!("not taint"),
        }
    }

    fn scan_py(source: &str) -> Vec<Finding> {
        let rule = python_rule();
        let parsed = crate::language::parse(Language::Python, source.as_bytes());
        taint_findings(&rule, Language::Python, &parsed.tree, source, "s.py", false)
    }

    #[test]
    fn reachable_flow_reports_with_evidence() {
        let findings = scan_py("def f():\n    eval(input(\"x\"))\n");
        assert_eq!(findings.len(), 1, "直接流应恰好一条：{findings:?}");
        assert_eq!(findings[0].span.start.line, 2);
        assert!(!findings[0].flows.is_empty(), "必须携带 source 位置");
    }

    #[test]
    fn assignment_chain_propagates() {
        let findings = scan_py("def f():\n    v = input(\"x\")\n    eval(v)\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].span.start.line, 3);
    }

    #[test]
    fn sanitizer_cuts_flow() {
        let findings = scan_py("def f():\n    v = int(input(\"x\"))\n    eval(v)\n");
        assert!(findings.is_empty(), "sanitizer 应切断：{findings:?}");
    }

    #[test]
    fn no_chain_and_cross_function_do_not_report() {
        assert!(scan_py("def f():\n    eval(\"1\")\n").is_empty());
        assert!(scan_py("def a():\n    v = input()\n\ndef b():\n    eval(v)\n").is_empty());
    }

    #[test]
    fn overlapping_node_kinds_do_not_duplicate() {
        // python 的 block / expression_statement / call 共享同一字节范围：
        // sink 判定同时要求节点种类相等，不得重复报告。
        let findings = scan_py("def f():\n    eval(input())\n");
        assert_eq!(findings.len(), 1);
    }
}
