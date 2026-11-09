//! 同文件过程间污点分析（Domain §4a v4）：函数索引 + 形参/实参映射 +
//! 布尔摘要不动点（支持递归）、返回值内部污点传播。变量名扁平化，
//! 发现 span = sink 命中；flows = 源头位置列表。跨文件不追踪。

use crate::compile::TaintRule;
use crate::language::Language;
use crate::metrics;
use crate::model::{Finding, Position, Span};
use std::collections::{BTreeMap, HashMap, HashSet};
use tree_sitter::{Node, StreamingIterator as _};

#[derive(Debug, Clone)]
struct Endpoint {
    start: usize,
    end: usize,
    /// @match 节点种类：sink 判定要求种类与字节范围双等值。
    kind: String,
    span: Span,
    /// `@match` 捕获为标识符形态时的扁平变量名；调用/表达式形态为 None。
    name: Option<String>,
    text: String,
}

#[derive(Debug, Clone)]
struct FnInfo<'tree> {
    node: Node<'tree>,
    name: Option<String>,
    param_names: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Summary {
    /// 形参索引 → 可达 sink（本函数体内或经同文件调用链）。
    sink_from_param: HashSet<usize>,
    /// 返回值可能被函数内部 source 污染时的首个 source 位置。
    ret_internal_flow: Option<Position>,
}

struct Analyzer<'a, 'tree> {
    rule: &'a TaintRule,
    lang: Language,
    source: &'a str,
    path: &'a str,
    degraded: bool,
    sinks: Vec<Endpoint>,
    sanitizer_ranges: Vec<(usize, usize)>,
    source_list: Vec<Endpoint>,
    fns: Vec<FnInfo<'tree>>,
    index: HashMap<String, Vec<usize>>,
    /// 端点归属函数（最内层包含者；顶层为 None）。
    sink_owner: Vec<Option<usize>>,
    source_owner: Vec<Option<usize>>,
    summaries: Vec<Summary>,
}

#[derive(Default)]
struct WalkState {
    /// 扁平变量名 → 源头位置。
    tainted: HashMap<String, Position>,
    /// 本函数内的 source 表达式范围。
    local_sources: Vec<(usize, usize, Position)>,
}

struct WalkCtx {
    expand_calls: bool,
    depth: usize,
    stack: Vec<(usize, usize)>,
}

#[derive(Default)]
struct WalkOutcome {
    sink_hit: bool,
    findings: Vec<Finding>,
}

const MAX_CALL_DEPTH: usize = 6;

pub fn taint_findings(
    rule: &TaintRule,
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> Vec<Finding> {
    let root = tree.root_node();
    let mut cursor = tree_sitter::QueryCursor::new();

    let source_list = collect_endpoints(&mut cursor, &rule.source_queries, root, source);
    let sinks = collect_endpoints(&mut cursor, &rule.sink_queries, root, source);
    let sanitizer_ranges: Vec<(usize, usize)> =
        collect_endpoints(&mut cursor, &rule.sanitizer_queries, root, source)
            .into_iter()
            .map(|e| (e.start, e.end))
            .collect();

    let fns = functions_of(root, lang, source);
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in fns.iter().enumerate() {
        if let Some(name) = &f.name {
            index.entry(name.clone()).or_default().push(i);
        }
    }

    let sink_owner: Vec<Option<usize>> = sinks
        .iter()
        .map(|s| owner_of(&fns, s.start, s.end))
        .collect();
    let source_owner: Vec<Option<usize>> = source_list
        .iter()
        .map(|s| owner_of(&fns, s.start, s.end))
        .collect();

    let mut analyzer = Analyzer {
        rule,
        lang,
        source,
        path,
        degraded,
        sinks,
        sanitizer_ranges,
        source_list,
        fns,
        index,
        sink_owner,
        source_owner,
        summaries: Vec::new(),
    };

    analyzer.compute_summaries();
    // 最终收集：每个函数以其内部 source（含作为 source 的形参标识符）为种子，
    // 沿同文件调用链递归收集 sink；同一 sink 合并 flows。
    let mut merged: BTreeMap<(usize, usize, usize, usize), Finding> = BTreeMap::new();
    for i in 0..analyzer.fns.len() {
        let mut state = WalkState::default();
        for (idx, e) in analyzer.source_list.iter().enumerate() {
            if analyzer.source_owner[idx] != Some(i) {
                continue;
            }
            state
                .local_sources
                .push((e.start, e.end, e.span.start));
            if let Some(name) = &e.name {
                state.tainted.entry(name.clone()).or_insert(e.span.start);
            }
        }
        let mut ctx = WalkCtx {
            expand_calls: true,
            depth: 0,
            stack: vec![],
        };
        let outcome = analyzer.walk_body(i, &mut state, &mut ctx);
        for finding in outcome.findings {
            let key = (
                finding.span.start.line,
                finding.span.start.column,
                finding.span.end.line,
                finding.span.end.column,
            );
            merged
                .entry(key)
                .and_modify(|existing| {
                    for flow in &finding.flows {
                        if !existing.flows.contains(flow) {
                            existing.flows.push(*flow);
                        }
                    }
                    existing.flows.sort_by_key(|p| (p.line, p.column));
                })
                .or_insert(finding);
        }
    }

    merged.into_values().collect()
}

// ---------- 摘要不动点 ----------

impl<'a, 'tree> Analyzer<'a, 'tree> {
    fn compute_summaries(&mut self) {
        let mut summaries: Vec<Summary> = vec![Summary::default(); self.fns.len()];
        let total_params: usize = self.fns.iter().map(|f| f.param_names.len()).sum();
        let max_iterations = total_params + self.fns.len() + 2;
        let mut iterations = 0;
        loop {
            let old = summaries.clone();
            let mut changed = false;
            // 返回值内部污点也参与不动点（调用链上的内部 source 逐轮外扩）。
            for (i, summary) in summaries.iter_mut().enumerate() {
                let flow = self.internal_return_flow(i, &old);
                if flow.is_some() && summary.ret_internal_flow.is_none() {
                    summary.ret_internal_flow = flow;
                    changed = true;
                }
            }
            for (i, summary) in summaries.iter_mut().enumerate() {
                for p in 0..self.fns[i].param_names.len() {
                    if self.fns[i].param_names[p].is_empty()
                        || summary.sink_from_param.contains(&p)
                    {
                        continue;
                    }
                    let mut state = WalkState::default();
                    state.tainted.insert(
                        self.fns[i].param_names[p].clone(),
                        Position { line: 1, column: 1 },
                    );
                    let mut ctx = WalkCtx {
                        expand_calls: false,
                        depth: 0,
                        stack: vec![(i, p)],
                    };
                    let outcome = self.walk_body_with(i, &mut state, &mut ctx, &old);
                    if outcome.sink_hit {
                        summary.sink_from_param.insert(p);
                        changed = true;
                    }
                }
            }
            iterations += 1;
            if !changed {
                break;
            }
            // 防御：单调布尔摘要必然在 total_params+2 轮内收敛。
            assert!(
                iterations <= max_iterations,
                "污点摘要未在 {max_iterations} 轮内收敛"
            );
        }
        self.summaries = summaries;
    }

    /// 函数内部 source 直接污染返回值的首个位置（显式 return 与表达式体/尾表达式）。
    fn internal_return_flow(&self, fn_idx: usize, summaries: &[Summary]) -> Option<Position> {
        let f = &self.fns[fn_idx];
        let mut state = WalkState::default();
        for (idx, e) in self.source_list.iter().enumerate() {
            if self.source_owner[idx] == Some(fn_idx) {
                state.local_sources.push((e.start, e.end, e.span.start));
            }
        }
        let mut first: Option<Position> = None;
        let mut check = |node: Node<'_>| {
            if first.is_some() {
                return;
            }
            if node.kind().contains("return") {
                let flows = self.flows_for_node(node, &state, summaries);
                if let Some(p) = flows.first() {
                    first = Some(*p);
                }
            }
        };
        walk_own(f, self.lang, &mut check);
        if first.is_some() {
            return first;
        }
        // 表达式体（箭头函数）与 Rust 尾表达式。
        if let Some(body) = f.node.child_by_field_name("body") {
            let body_is_block = body.kind() == "block" || body.kind() == "statement_block";
            if !body_is_block {
                return self.flows_for_node(body, &state, summaries).first().copied();
            }
            if let Some(last) = named_children(body).last() {
                if !matches!(
                    last.kind(),
                    "expression_statement" | "let_declaration" | "return_statement" | "return_expression"
                ) {
                    return self.flows_for_node(*last, &state, summaries).first().copied();
                }
            }
        }
        None
    }

    fn walk_body(
        &self,
        fn_idx: usize,
        state: &mut WalkState,
        ctx: &mut WalkCtx,
    ) -> WalkOutcome {
        // 最终收集阶段使用已收敛摘要。
        let summaries = self.summaries.clone();
        self.walk_body_with(fn_idx, state, ctx, &summaries)
    }

    fn walk_body_with(
        &self,
        fn_idx: usize,
        state: &mut WalkState,
        ctx: &mut WalkCtx,
        summaries: &[Summary],
    ) -> WalkOutcome {
        let mut outcome = WalkOutcome::default();
        let f = &self.fns[fn_idx];
        for child in named_children(f.node) {
            if self.is_function(child) {
                continue;
            }
            self.visit(child, fn_idx, state, ctx, summaries, &mut outcome);
        }
        outcome
    }

    fn is_function(&self, node: Node<'_>) -> bool {
        metrics::profile(self.lang)
            .functions
            .contains(&node.kind())
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        &self,
        node: Node<'_>,
        fn_idx: usize,
        state: &mut WalkState,
        ctx: &mut WalkCtx,
        summaries: &[Summary],
        out: &mut WalkOutcome,
    ) {
        let (ns, ne) = (node.start_byte(), node.end_byte());

        // 1) 赋值传播。
        let assignment = metrics::profile(self.lang)
            .assignments
            .iter()
            .find(|(kind, _, _)| *kind == node.kind())
            .copied();
        if let Some((_, lhs_field, rhs_field)) = assignment {
            if let (Some(lhs), Some(rhs)) = (
                node.child_by_field_name(lhs_field),
                node.child_by_field_name(rhs_field),
            ) {
                if let Some(flow) = self.rhs_taint(rhs, state, summaries) {
                    for (name, _) in identifiers_within(lhs, self.source) {
                        state.tainted.entry(name).or_insert(flow);
                    }
                }
            }
        }

        // 2) 调用点：实参污点 → 被调函数形参 → sink 可达 / 递归收集。
        if let Some((callee, args)) = call_info(node, self.lang, self.source) {
            if let Some(callee_indices) = self.index.get(&callee).cloned() {
                for (arg_idx, arg) in args.iter().enumerate() {
                    let Some(flow) = self.rhs_taint(*arg, state, summaries) else {
                        continue;
                    };
                    for callee_idx in &callee_indices {
                        if !summaries[*callee_idx].sink_from_param.contains(&arg_idx) {
                            continue;
                        }
                        out.sink_hit = true;
                        if !ctx.expand_calls || ctx.depth >= MAX_CALL_DEPTH {
                            continue;
                        }
                        let Some(param_name) = self.fns[*callee_idx].param_names.get(arg_idx)
                        else {
                            continue;
                        };
                        if param_name.is_empty()
                            || ctx.stack.contains(&(*callee_idx, arg_idx))
                        {
                            continue;
                        }
                        let mut callee_state = WalkState::default();
                        callee_state.tainted.insert(param_name.clone(), flow);
                        let mut next_ctx = WalkCtx {
                            expand_calls: true,
                            depth: ctx.depth + 1,
                            stack: {
                                let mut stack = ctx.stack.clone();
                                stack.push((*callee_idx, arg_idx));
                                stack
                            },
                        };
                        let sub = self.walk_body_with(
                            *callee_idx,
                            &mut callee_state,
                            &mut next_ctx,
                            summaries,
                        );
                        out.findings.extend(sub.findings);
                    }
                }
            }
        }

        // 3) sink 命中（种类 + 字节范围双等值，且属于当前函数）。
        if let Some(sink) = self
            .sinks
            .iter()
            .enumerate()
            .find(|(idx, s)| {
                self.sink_owner[*idx] == Some(fn_idx)
                    && s.start == ns
                    && s.end == ne
                    && s.kind == node.kind()
            })
            .map(|(_, s)| s)
        {
            let sanitized = self
                .sanitizer_ranges
                .iter()
                .any(|(ss, se)| *ss <= sink.start && *se >= sink.end);
            if !sanitized {
                let flows = self.flows_for_node(node, state, summaries);
                if !flows.is_empty() {
                    out.sink_hit = true;
                    if ctx.expand_calls {
                        out.findings.push(Finding {
                            rule_id: self.rule.spec.id.clone(),
                            dimension: self.rule.spec.dimension,
                            severity: self.rule.spec.severity,
                            path: self.path.to_string(),
                            span: sink.span,
                            message: self.rule.spec.message.clone(),
                            match_text: sink.text.clone(),
                            captures: Vec::new(),
                            degraded: self.degraded,
                            flows,
                            fingerprint: String::new(),
                        });
                    }
                }
            }
        }

        for child in named_children(node) {
            if self.is_function(child) {
                continue;
            }
            self.visit(child, fn_idx, state, ctx, summaries, out);
        }
    }

    /// 表达式级污点：sanitizer 覆盖 → None；source 范围/污点标识符/内部返回污点调用。
    fn rhs_taint(
        &self,
        node: Node<'_>,
        state: &WalkState,
        summaries: &[Summary],
    ) -> Option<Position> {
        self.flows_for_node(node, state, summaries).first().copied()
    }

    fn flows_for_node(
        &self,
        node: Node<'_>,
        state: &WalkState,
        summaries: &[Summary],
    ) -> Vec<Position> {
        let mut flows: Vec<Position> = Vec::new();
        let (ns, ne) = (node.start_byte(), node.end_byte());
        // source 范围（未被 sanitizer 覆盖的部分整体判定）。
        for (s0, s1, pos) in &state.local_sources {
            if ns <= *s0 && *s1 <= ne && !self.range_in_sanitizer(*s0, *s1) {
                flows.push(*pos);
            }
        }
        // 污点标识符（落在 sanitizer 范围内视为清洗）。
        for ident in identifiers_within_nodes(node) {
            let (is_, ie_) = (ident.start_byte(), ident.end_byte());
            if self.range_in_sanitizer(is_, ie_) {
                continue;
            }
            let text = self
                .source
                .get(is_..ie_)
                .unwrap_or("")
                .to_string();
            if let Some(pos) = state.tainted.get(&text) {
                flows.push(*pos);
            }
        }
        // 调用返回内部污点。
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if let Some((callee, _args)) = call_info(current, self.lang, self.source) {
                if let Some(indices) = self.index.get(&callee) {
                    for idx in indices {
                        if let Some(pos) = summaries[*idx].ret_internal_flow {
                            if !self.range_in_sanitizer(current.start_byte(), current.end_byte()) {
                                flows.push(pos);
                            }
                        }
                    }
                }
            }
            for child in named_children(current) {
                if self.is_function(child) {
                    continue;
                }
                stack.push(child);
            }
        }
        flows.sort_by_key(|p| (p.line, p.column));
        flows.dedup();
        flows
    }

    fn range_in_sanitizer(&self, start: usize, end: usize) -> bool {
        self.sanitizer_ranges
            .iter()
            .any(|(ss, se)| start >= *ss && end <= *se)
    }
}

// ---------- 端点与函数索引 ----------

fn collect_endpoints(
    cursor: &mut tree_sitter::QueryCursor,
    queries: &[tree_sitter::Query],
    root: Node<'_>,
    source: &str,
) -> Vec<Endpoint> {
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
                // 多 @match 时取最小跨度节点（端点应是参数标识符而非整个函数）。
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

fn functions_of<'tree>(
    root: Node<'tree>,
    lang: Language,
    source: &str,
) -> Vec<FnInfo<'tree>> {
    let prof = metrics::profile(lang);
    let mut out = Vec::new();
    metrics::walk_all_pub(root, &mut |node| {
        if prof.functions.contains(&node.kind()) {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                .map(|s| s.to_string());
            let param_names = param_names(node, lang, source);
            out.push(FnInfo {
                node,
                name,
                param_names,
            });
        }
    });
    // 确定性：按起点排序（收集顺序本身是树序，显式排序防未来改动）。
    out.sort_by_key(|f| f.node.start_byte());
    out
}

fn owner_of(fns: &[FnInfo<'_>], start: usize, end: usize) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, f) in fns.iter().enumerate() {
        let (fs, fe) = (f.node.start_byte(), f.node.end_byte());
        if fs <= start && end <= fe {
            let better = best
                .map(|b| {
                    let (bs, be) = (fns[b].node.start_byte(), fns[b].node.end_byte());
                    (fe - fs) < (be - bs)
                })
                .unwrap_or(true);
            if better {
                best = Some(i);
            }
        }
    }
    best
}

fn param_names(node: Node<'_>, lang: Language, source: &str) -> Vec<String> {
    let Some(params) = node.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let children = named_children(params);
    let mut names = Vec::new();
    match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
            for child in children {
                match child.kind() {
                    "identifier" | "required_parameter" | "optional_parameter" => {
                        names.push(
                            first_identifier(&child, source).unwrap_or_default(),
                        );
                    }
                    "rest_pattern" | "object_pattern" | "array_pattern" => {
                        names.push(String::new())
                    }
                    _ => names.push(String::new()),
                }
            }
        }
        Language::Python => {
            for child in children {
                match child.kind() {
                    "identifier" => names.push(
                        source
                            .get(child.start_byte()..child.end_byte())
                            .unwrap_or("")
                            .to_string(),
                    ),
                    "typed_parameter" | "default_parameter" => {
                        names.push(first_identifier(&child, source).unwrap_or_default())
                    }
                    "list_splat_pattern" | "dictionary_splat_pattern" => {
                        names.push(String::new())
                    }
                    "keyword_separator" | "positional_separator" => {}
                    _ => names.push(first_identifier(&child, source).unwrap_or_default()),
                }
            }
        }
        Language::Java => {
            for child in children {
                if child.kind() == "formal_parameter" {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                        .map(|s| s.to_string())
                        .or_else(|| first_identifier(&child, source));
                    names.push(name.unwrap_or_default());
                }
            }
        }
        Language::Go => {
            for child in children {
                if child.kind() == "parameter_declaration" {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                        .map(|s| s.to_string());
                    names.push(name.unwrap_or_default());
                }
            }
        }
        Language::Rust => {
            for child in children {
                match child.kind() {
                    "parameter" => names.push(
                        child
                            .child_by_field_name("pattern")
                            .map(|p| first_identifier(&p, source).unwrap_or_default())
                            .unwrap_or_default(),
                    ),
                    "self_parameter" => names.push(String::new()),
                    _ => names.push(String::new()),
                }
            }
        }
    }
    names
}

fn first_identifier(node: &Node<'_>, source: &str) -> Option<String> {
    let mut found: Option<String> = None;
    metrics::walk_all_pub(*node, &mut |n| {
        if found.is_none() && n.kind().contains("identifier") {
            found = source
                .get(n.start_byte()..n.end_byte())
                .map(|s| s.to_string());
        }
    });
    found
}

/// 调用点解析：返回 (被调名, 位置参数节点)。
fn call_info<'tree>(node: Node<'tree>, lang: Language, source: &str) -> Option<(String, Vec<Node<'tree>>)> {
    let (callee_node, args_node) = match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        | Language::Go | Language::Rust => {
            if node.kind() != "call_expression" {
                return None;
            }
            (
                node.child_by_field_name("function")?,
                node.child_by_field_name("arguments")?,
            )
        }
        Language::Python => {
            if node.kind() != "call" {
                return None;
            }
            (
                node.child_by_field_name("function")?,
                node.child_by_field_name("arguments")?,
            )
        }
        Language::Java => {
            if node.kind() != "method_invocation" {
                return None;
            }
            (
                node.child_by_field_name("name")?,
                node.child_by_field_name("arguments")?,
            )
        }
    };
    let callee = match callee_node.kind() {
        k if k.contains("identifier") || k == "field_identifier" => source
            .get(callee_node.start_byte()..callee_node.end_byte())
            .map(|s| s.to_string()),
        "member_expression" => callee_node
            .child_by_field_name("property")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "field_expression" => callee_node
            .child_by_field_name("field")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "attribute" => callee_node
            .child_by_field_name("attribute")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "scoped_identifier" => callee_node
            .child_by_field_name("name")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        _ => None,
    }?;
    if callee.is_empty() {
        return None;
    }
    let args = named_children(args_node)
        .into_iter()
        .filter(|a| !matches!(a.kind(), "keyword_argument" | "comment"))
        .collect();
    Some((callee, args))
}

// ---------- 通用节点/文本工具 ----------

fn named_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'tree>> = node.named_children(&mut cursor).collect();
    drop(cursor);
    children
}

fn walk_own<'tree, F: FnMut(Node<'tree>)>(f: &FnInfo<'tree>, lang: Language, f_visit: &mut F) {
    let prof = metrics::profile(lang);
    fn rec<'tree>(
        node: Node<'tree>,
        functions: &[&str],
        visit: &mut impl FnMut(Node<'tree>),
    ) {
        visit(node);
        for child in named_children(node) {
            if functions.contains(&child.kind()) {
                continue;
            }
            rec(child, functions, visit);
        }
    }
    for child in named_children(f.node) {
        if prof.functions.contains(&child.kind()) {
            continue;
        }
        rec(child, prof.functions, f_visit);
    }
}

fn identifiers_within_nodes<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::new();
    fn rec<'tree>(node: Node<'tree>, out: &mut Vec<Node<'tree>>) {
        if node.kind().contains("identifier") || node.kind() == "attribute" {
            out.push(node);
        }
        for child in named_children(node) {
            rec(child, out);
        }
    }
    rec(node, &mut out);
    out
}

fn identifiers_within(node: Node<'_>, source: &str) -> Vec<(String, Position)> {
    let mut out = Vec::new();
    for n in identifiers_within_nodes(node) {
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
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile, CompiledRule};
    use crate::rules::parse_rule_set;

    fn python_rule(kind: &str) -> TaintRule {
        let sources = if kind == "input" {
            "(call\n            function: (identifier) @fn (#eq? @fn \"input\")) @match"
        } else {
            "(function_definition\n            parameters: (parameters (identifier) @match)) @match"
        };
        let yaml = format!(
            "rules:\n  - id: probe/taint\n    language: python\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          {}\n      sinks:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n      sanitizers:\n        - |\n          (call\n            function: (identifier) @fn (#eq? @fn \"int\")) @match\n",
            sources
        );
        let specs = parse_rule_set(&yaml).expect("parse");
        let compiled = compile(specs).expect("compile");
        match compiled.into_iter().next().unwrap() {
            CompiledRule::Taint(r) => r,
            _ => panic!("not taint"),
        }
    }

    fn scan(kind: &str, source: &str) -> Vec<Finding> {
        let rule = python_rule(kind);
        let parsed = crate::language::parse(Language::Python, source.as_bytes());
        let mut findings = taint_findings(
            &rule,
            Language::Python,
            &parsed.tree,
            source,
            "s.py",
            false,
        );
        findings.sort_by_key(|f| (f.span.start.line, f.span.start.column));
        findings
    }

    #[test]
    fn reachable_flow_reports_with_evidence() {
        let findings = scan("input", "def f():\n    eval(input(\"x\"))\n");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].span.start.line, 2);
        assert!(!findings[0].flows.is_empty());
    }

    #[test]
    fn assignment_chain_propagates() {
        let findings = scan("input", "def f():\n    v = input(\"x\")\n    eval(v)\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].span.start.line, 3);
    }

    #[test]
    fn sanitizer_cuts_flow() {
        assert!(scan("input", "def f():\n    v = int(input(\"x\"))\n    eval(v)\n").is_empty());
    }

    #[test]
    fn no_chain_and_cross_function_param_isolation() {
        // 函数 B 的参数在 input-source 规则下不是 source；A 的变量不可见。
        assert!(scan("input", "def f():\n    eval(\"1\")\n").is_empty());
        assert!(scan("input", "def a():\n    v = input()\n\ndef b():\n    eval(v)\n").is_empty());
    }

    #[test]
    fn overlapping_node_kinds_do_not_duplicate() {
        assert_eq!(scan("input", "def f():\n    eval(input())\n").len(), 1);
    }

    // ---- v4：同文件过程间 ----

    #[test]
    fn d1_call_chain_reaches_sink_with_caller_flow() {
        let source = "def handle(user_input):\n    run(user_input)\n\ndef run(code):\n    eval(code)\n";
        let findings = scan("param", source);
        let sink: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.span.start.line == 5)
            .collect();
        assert_eq!(sink.len(), 1, "sink 应恰一条：{findings:?}");
        // flows 至少包含调用者参数位置（第 1 行）。
        assert!(
            sink[0].flows.iter().any(|p| p.line == 1),
            "flows 应包含调用者源头：{:?}",
            sink[0].flows
        );
    }

    #[test]
    fn d2_return_value_flow() {
        // 调用返回值携带内部 source。
        let source = "def helper():\n    return input()\n\ndef f():\n    v = helper()\n    eval(v)\n";
        let findings = scan("input", source);
        assert!(
            findings.iter().any(|f| f.span.start.line == 6),
            "返回值流应命中：{findings:?}"
        );
    }

    #[test]
    fn d3_sanitizer_at_call_boundary() {
        let source = "def run(code):\n    eval(code)\n\ndef safe(v):\n    run(int(v))\n";
        let findings = scan("param", source);
        // run 自身参数是 source（v2 语义），其自身命中仍存在；但调用边界不应引入 v 的流。
        let sink = findings
            .iter()
            .find(|f| f.span.start.line == 2)
            .expect("run 自身参数作为 source 命中");
        assert!(
            !sink.flows.iter().any(|p| p.line == 5),
            "int() 之后不应有 safe 参数流：{:?}",
            sink.flows
        );
    }

    #[test]
    fn d4_recursion_terminates_and_is_deterministic() {
        let source = "def a(x):\n    b(x)\n\ndef b(y):\n    eval(y)\n    a(y)\n";
        let first = scan("param", source);
        let second = scan("param", source);
        assert!(!first.is_empty());
        assert_eq!(
            format!("{:?}", first.iter().map(|f| (f.span.start.line, &f.flows)).collect::<Vec<_>>()),
            format!("{:?}", second.iter().map(|f| (f.span.start.line, &f.flows)).collect::<Vec<_>>()),
        );
    }

    #[test]
    fn d6_same_name_union() {
        let source = "def helper(x):\n    eval(x)\n\ndef helper(y):\n    print(y)\n\ndef entry(v):\n    helper(v)\n";
        let findings = scan("param", source);
        assert!(
            findings.iter().any(|f| f.span.start.line == 2),
            "同名函数并集应命中：{findings:?}"
        );
    }
}
