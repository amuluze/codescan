//! 五项内置度量（quality 维度，走统一发现管线与指纹规则）。
//! 语义均为 v1 近似（启发式），消息不声称超出结构匹配的证明力。

use crate::compile::MetricKind;
use crate::language::Language;
use crate::model::{Dimension, Finding, Position, Severity, Span};
use std::collections::HashMap;
use tree_sitter::Node;

const METRIC_SEVERITY: Severity = Severity::Warning;
const METRIC_DIMENSION: Dimension = Dimension::Quality;

/// 各语法 node 名档位。未知 node 类型不计数（宁少报不误报）。
pub(crate) struct Profile {
    pub(crate) functions: &'static [&'static str],
    pub(crate) blocks: &'static [&'static str],
    pub(crate) terminal: &'static [&'static str],
    pub(crate) decisions: &'static [&'static str],
    /// binary/boolean 运算节点名（按运算符文本再筛）。
    pub(crate) logic_ops: &'static [&'static str],
    /// 语句序列的直接子节点若是包装层（Java 的 block_statement），经它取内层。
    pub(crate) statement_wrapper: Option<&'static str>,
    /// 赋值/声明传播节点：(kind, lhs 字段, rhs 字段)。污点分析用（Domain §4a）。
    pub(crate) assignments: &'static [(&'static str, &'static str, &'static str)],
}

pub(crate) fn profile(lang: Language) -> Profile {
    match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => Profile {
            functions: &[
                "function_declaration",
                "generator_function_declaration",
                "function_expression",
                "generator_function",
                "arrow_function",
                "method_definition",
            ],
            blocks: &["statement_block"],
            terminal: &[
                "return_statement",
                "throw_statement",
                "break_statement",
                "continue_statement",
            ],
            decisions: &[
                "if_statement",
                "for_statement",
                "for_in_statement",
                "while_statement",
                "do_statement",
                "switch_case",
                "catch_clause",
                "ternary_expression",
            ],
            logic_ops: &["binary_expression"],
                        statement_wrapper: None,
            assignments: &[
                ("assignment_expression", "left", "right"),
                ("variable_declarator", "name", "value"),
                ("augmented_assignment_expression", "left", "right"),
            ],
        },
        Language::Python => Profile {
            functions: &["function_definition"],
            blocks: &["block"],
            terminal: &[
                "return_statement",
                "raise_statement",
                "break_statement",
                "continue_statement",
            ],
            decisions: &[
                "if_statement",
                "elif_clause",
                "while_statement",
                "for_statement",
                "except_clause",
                "conditional_expression",
                "boolean_operator",
            ],
            logic_ops: &["boolean_operator"],
                        statement_wrapper: None,
            assignments: &[("assignment", "left", "right")],
        },
        Language::Java => Profile {
            functions: &["method_declaration", "constructor_declaration"],
            blocks: &["block"],
            terminal: &[
                "return_statement",
                "throw_statement",
                "break_statement",
                "continue_statement",
            ],
            decisions: &[
                "if_statement",
                "for_statement",
                "enhanced_for_statement",
                "while_statement",
                "do_statement",
                "switch_block_statement_group",
                "catch_clause",
                "ternary_expression",
            ],
            logic_ops: &["binary_expression"],
                        statement_wrapper: Some("block_statement"),
            assignments: &[
                ("assignment_expression", "lhs", "value"),
                ("variable_declarator", "name", "value"),
            ],
        },
        Language::Go => Profile {
            functions: &["function_declaration", "method_declaration"],
            blocks: &["block"],
            terminal: &["return_statement", "break_statement", "continue_statement"],
            decisions: &[
                "if_statement",
                "for_statement",
                "expression_case",
                "communication_case",
                "type_case",
            ],
            logic_ops: &["binary_expression"],
                        statement_wrapper: None,
            assignments: &[
                ("assignment_statement", "left", "right"),
                ("short_var_declaration", "left", "right"),
            ],
        },
        Language::Rust => Profile {
            functions: &["function_item"],
            blocks: &["block"],
            terminal: &[
                "return_expression",
                "break_expression",
                "continue_expression",
                "throw_expression",
            ],
            decisions: &[
                "if_expression",
                "if_let_expression",
                "while_expression",
                "while_let_expression",
                "for_expression",
                "match_arm",
            ],
            logic_ops: &["binary_expression"],
                        statement_wrapper: None,
            assignments: &[
                ("let_declaration", "name", "value"),
                ("assignment_expression", "left", "value"),
            ],
        },
    }
}

fn option(options: &[(String, f64)], name: &str, default: f64) -> f64 {
    options
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

fn finding(rule_id: &str, message: String, path: &str, span: Span, match_text: &str, degraded: bool) -> Finding {
    Finding {
        rule_id: rule_id.into(),
        dimension: METRIC_DIMENSION,
        severity: METRIC_SEVERITY,
        path: path.into(),
        span,
        message,
        match_text: match_text.into(),
        captures: Vec::new(),
        degraded,
        flows: Vec::new(),
        fingerprint: String::new(),
    }
}

fn span_of(node: Node<'_>) -> Span {
    let start = node.start_position();
    let end = node.end_position();
    Span {
        start: Position {
            line: start.row + 1,
            column: start.column + 1,
        },
        end: Position {
            line: end.row + 1,
            column: end.column + 1,
        },
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    let start = node.start_byte();
    let end = node.end_byte();
    source.get(start..end).unwrap_or("")
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    drop(cursor);
    children
}

/// 遍历子树（含自身）的所有命名后代。taint.rs 经 walk_all_pub 复用。
pub(crate) fn walk_all_pub<'tree, F: FnMut(tree_sitter::Node<'tree>)>(
    root: tree_sitter::Node<'tree>,
    f: &mut F,
) {
    walk_all(root, f)
}

fn walk_all<'tree, F: FnMut(tree_sitter::Node<'tree>)>(
    root: tree_sitter::Node<'tree>,
    f: &mut F,
) {
    let mut cursor = root.walk();
    let mut reached_root = false;
    f(cursor.node());
    while !reached_root {
        if cursor.goto_first_child() {
            f(cursor.node());
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                f(cursor.node());
                break;
            }
            if !cursor.goto_parent() {
                reached_root = true;
                break;
            }
        }
    }
}

fn is_logic_operator_node(node: Node<'_>, source: &str, lang: Language) -> bool {
    let prof = profile(lang);
    if !prof.logic_ops.contains(&node.kind()) {
        return false;
    }
    if lang == Language::Python {
        return true; // boolean_operator 每个和/或各计一次
    }
    // 其余语言：子节点文本命中逻辑运算符集合。
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
    drop(cursor);
    children
        .iter()
        .any(|child| matches!(node_text(*child, source), "&&" | "||" | "??"))
}

fn function_name(func: Node<'_>, source: &str) -> String {
    // 各语法函数节点均以 `name` 字段携带标识符（arrow function 例外）。
    match func.child_by_field_name("name") {
        Some(name) => node_text(name, source).to_string(),
        None => "(匿名)".into(),
    }
}

/// 计算文件级度量（复杂度/长度/嵌套/不可达）。重复块为语料级，另行计算。
pub fn file_metrics(
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
    configs: &[(MetricKind, Vec<(String, f64)>)],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let root = tree.root_node();
    let prof = profile(lang);

    let cyclomatic_threshold = configs
        .iter()
        .find(|(k, _)| *k == MetricKind::CyclomaticComplexity)
        .map(|(_, o)| option(o, "threshold", 15.0))
        .unwrap_or(15.0);
    let length_threshold = configs
        .iter()
        .find(|(k, _)| *k == MetricKind::FunctionLength)
        .map(|(_, o)| option(o, "threshold", 80.0))
        .unwrap_or(80.0);
    let nesting_threshold = configs
        .iter()
        .find(|(k, _)| *k == MetricKind::NestingDepth)
        .map(|(_, o)| option(o, "threshold", 5.0))
        .unwrap_or(5.0);

    // 函数级度量。
    let has_length = configs
        .iter()
        .any(|(k, _)| *k == MetricKind::FunctionLength);
    let has_cyclomatic = configs
        .iter()
        .any(|(k, _)| *k == MetricKind::CyclomaticComplexity);
    let has_nesting = configs.iter().any(|(k, _)| *k == MetricKind::NestingDepth);

    if has_length || has_cyclomatic || has_nesting {
        let mut functions = Vec::new();
        walk_all(root, &mut |node| {
            if prof.functions.contains(&node.kind()) {
                functions.push(node);
            }
        });
        for func in functions {
            // 无函数体（抽象方法/声明）不参与度量。
            let has_body = named_children(func).iter().any(|c| prof.blocks.contains(&c.kind()));
            if !has_body {
                continue;
            }
            let span = span_of(func);
            let lines = span.end.line - span.start.line + 1;
            let name = function_name(func, source);
            let text = node_text(func, source).to_string();

            if has_length && lines as f64 > length_threshold {
                findings.push(finding(
                    "metric/function-length",
                    format!("函数 {name} 长度 {lines} 行，超过阈值 {length_threshold}"),
                    path,
                    span,
                    &text,
                    degraded,
                ));
            }
            if has_cyclomatic {
                let mut decisions = 0usize;
                walk_all(func, &mut |node| {
                    if prof.decisions.contains(&node.kind())
                        || is_logic_operator_node(node, source, lang)
                    {
                        decisions += 1;
                    }
                });
                let complexity = 1 + decisions;
                if complexity as f64 > cyclomatic_threshold {
                    findings.push(finding(
                        "metric/cyclomatic-complexity",
                        format!(
                            "函数 {name} 圈复杂度 {complexity}，超过阈值 {cyclomatic_threshold}"
                        ),
                        path,
                        span,
                        &text,
                        degraded,
                    ));
                }
            }
            if has_nesting {
                let depth = max_block_depth(func, prof.blocks) - 1; // 函数体自身为 0 层
                if depth as f64 > nesting_threshold {
                    findings.push(finding(
                        "metric/nesting-depth",
                        format!("函数 {name} 最大嵌套深度 {depth}，超过阈值 {nesting_threshold}"),
                        path,
                        span,
                        &text,
                        degraded,
                    ));
                }
            }
        }
    }

    if configs.iter().any(|(k, _)| *k == MetricKind::UnreachableCode) {
        findings.extend(unreachable_findings(lang, root, source, path, degraded, &prof));
    }

    findings
}

fn max_block_depth(func: Node<'_>, blocks: &[&str]) -> usize {
    fn rec(node: Node<'_>, blocks: &[&str], depth: usize, max: &mut usize) {
        let next = if blocks.contains(&node.kind()) {
            depth + 1
        } else {
            depth
        };
        if next > *max {
            *max = next;
        }
        for child in named_children(node) {
            rec(child, blocks, next, max);
        }
    }
    let mut max = 0usize;
    for child in named_children(func) {
        rec(child, blocks, 0, &mut max);
    }
    max.max(1) // 函数体块至少算 1（供外部 -1）
}

fn is_terminal_statement(node: Node<'_>, prof: &Profile, source: &str, lang: Language) -> bool {
    if prof.terminal.contains(&node.kind()) {
        return true;
    }
    match lang {
        // Go: panic 调用的表达式语句视为终止。
        Language::Go if node.kind() == "expression_statement" => {
            let mut is_panic = false;
            walk_all(node, &mut |n| {
                if n.kind() == "call_expression" {
                    if let Some(fname) = n.child_by_field_name("function") {
                        if node_text(fname, source) == "panic" {
                            is_panic = true;
                        }
                    }
                }
            });
            is_panic
        }
        // Rust: panic!/unreachable!/todo!/unimplemented! 宏表达式语句视为终止。
        Language::Rust if node.kind() == "expression_statement" => {
            let mut is_panic = false;
            walk_all(node, &mut |n| {
                if n.kind() == "macro_invocation" {
                    let text = node_text(n, source);
                    let head = text.split('(').next().unwrap_or("");
                    let bare = head.trim_end_matches('!').rsplit("::").next().unwrap_or(head);
                    if matches!(bare, "panic" | "unreachable" | "todo" | "unimplemented") {
                        is_panic = true;
                    }
                }
            });
            is_panic
        }
        _ => false,
    }
}

fn unreachable_findings(
    lang: Language,
    root: Node<'_>,
    _source: &str,
    path: &str,
    degraded: bool,
    prof: &Profile,
) -> Vec<Finding> {
    let _ = lang;
    let mut findings = Vec::new();
    let mut blocks = Vec::new();
    walk_all(root, &mut |node| {
        if prof.blocks.contains(&node.kind()) {
            blocks.push(node);
        }
    });
    for block in blocks {
        let children = named_children(block);
        let mut terminal_seen = false;
        for child in children {
            // 注释是 extra 节点，不占语句序位（否则「return 后注释」会顶替首个不可达语句）。
            if child.kind() == "comment" {
                continue;
            }
            let stmt = match prof.statement_wrapper {
                Some(wrapper) if child.kind() == wrapper => {
                    match child.named_child(0) {
                        Some(inner) => inner,
                        None => continue,
                    }
                }
                Some(_) => continue, // 序列里只认包装层（Java block_statement）
                None => child,
            };
            if terminal_seen {
                let span = span_of(child);
                findings.push(finding(
                    "metric/unreachable-code",
                    "终止语句后存在不可达代码".into(),
                    path,
                    span,
                    "",
                    degraded,
                ));
                break; // 每个块只报第一条不可达区域
            }
            if is_terminal_statement(stmt, prof, _source, lang_for_terminal(prof)) {
                terminal_seen = true;
            }
        }
    }
    findings
}

fn lang_for_terminal(prof: &Profile) -> Language {
    // 由 terminal 集合反推语言（避免再传参数）；仅用于 panic 特判。
    if prof.terminal.contains(&"raise_statement") {
        Language::Python
    } else if prof.terminal.contains(&"throw_expression") {
        Language::Rust
    } else if prof.decisions.contains(&"enhanced_for_statement") {
        Language::Java
    } else if prof.decisions.contains(&"communication_case") {
        Language::Go
    } else {
        Language::TypeScript
    }
}

/// 跨文件重复块（语料级）。每处副本各发一条发现（Domain §1 多位置度量）。
pub fn duplicate_blocks(
    corpus: &[(String, Vec<(String, usize)>)], // (path, [(normalized_line, original_line)])
    configs: &[(MetricKind, Vec<(String, f64)>)],
) -> Vec<Finding> {
    let window = configs
        .iter()
        .find(|(k, _)| *k == MetricKind::DuplicateBlock)
        .map(|(_, o)| option(o, "window", 10.0))
        .unwrap_or(10.0) as usize;
    let min_occurrences = configs
        .iter()
        .find(|(k, _)| *k == MetricKind::DuplicateBlock)
        .map(|(_, o)| option(o, "min_occurrences", 2.0))
        .unwrap_or(2.0) as usize;
    if window == 0 {
        return Vec::new();
    }

    // 窗口哈希 → 出现位置列表。
    let mut groups: HashMap<u64, Vec<(usize, usize)>> = HashMap::new(); // hash -> [(file_idx, seq_idx)]
    for (file_idx, (_, lines)) in corpus.iter().enumerate() {
        if lines.len() < window {
            continue;
        }
        for seq_idx in 0..=(lines.len() - window) {
            let mut hasher = Sha256Hasher::default();
            for (text, _) in &lines[seq_idx..seq_idx + window] {
                hasher.update(text.as_bytes());
                    hasher.update(&[0xFF]);
            }
            groups
                .entry(hasher.finalize64())
                .or_default()
                .push((file_idx, seq_idx));
        }
    }

    let mut groups_vec: Vec<(&u64, &Vec<(usize, usize)>)> = groups.iter().collect();
    groups_vec.sort_by_key(|(hash, _)| **hash); // 确定性：按哈希排序

    // Pass 2：每组先做文件内相邻窗口合并；合格组（≥min_occurrences 个副本）
    // 的区间再按文件做跨组重叠合并，消除同一重复区域的错位多报。
    let mut per_file_intervals: HashMap<usize, Vec<(usize, usize)>> = HashMap::new(); // file_idx -> [seq_start, seq_end_exclusive]
    for (_hash, occurrences) in groups_vec {
        if occurrences.len() < min_occurrences {
            continue;
        }
        let mut runs: Vec<(usize, usize, usize)> = Vec::new(); // (file_idx, first_seq, last_seq)
        for (file_idx, seq_idx) in occurrences {
            match runs.last_mut() {
                Some((f, _, last)) if *f == *file_idx && *last + 1 == *seq_idx => {
                    *last = *seq_idx;
                }
                _ => runs.push((*file_idx, *seq_idx, *seq_idx)),
            }
        }
        if runs.len() < min_occurrences {
            continue;
        }
        for (file_idx, first, last) in runs {
            per_file_intervals.entry(file_idx).or_default().push((first, last + window));
        }
    }

    let mut findings = Vec::new();
    for (file_idx, (path, lines)) in corpus.iter().enumerate() {
        let Some(mut intervals) = per_file_intervals.remove(&file_idx) else {
            continue;
        };
        intervals.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in intervals {
            match merged.last_mut() {
                Some((_, prev_end)) if start <= *prev_end => {
                    *prev_end = (*prev_end).max(end);
                }
                _ => merged.push((start, end)),
            }
        }
        for (start, end) in merged {
            let start_line = lines[start].1;
            let end_line = lines[end - 1].1;
            let block_text = lines[start..end]
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            findings.push(finding(
                "metric/duplicate-block",
                format!("重复代码块：约 {} 行（出现于多个位置，启发式）", end - start),
                path,
                Span {
                    start: Position {
                        line: start_line,
                        column: 1,
                    },
                    end: Position {
                        line: end_line,
                        column: 1,
                    },
                },
                &block_text,
                false,
            ));
        }
    }
    findings
}

/// FNV-1a 64 位（窗口去重的快速哈希；碰撞再由内容哈希兜底不敏感：仅提示性度量）。
#[derive(Default)]
struct Sha256Hasher {
    state: u64,
}

impl Sha256Hasher {
    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= b as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finalize64(&self) -> u64 {
        self.state
    }
}
