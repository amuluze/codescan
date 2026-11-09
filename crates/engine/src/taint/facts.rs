//! 事实提取 IR：把（规则, 文件）的污点相关结构压缩为可跨文件分析的事实，
//! 不保留语法树。字段敏感由访问路径表达；对象字面量按字面键展开。

use super::{collect_endpoints, named_children, Endpoint};
use crate::compile::TaintRule;
use crate::language::Language;
use crate::metrics;
use crate::model::{Position, Span};
use tree_sitter::Node;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccessPath {
    pub base: String,
    pub fields: Vec<String>,
}

impl AccessPath {
    pub fn bare(base: impl Into<String>) -> Self {
        AccessPath {
            base: base.into(),
            fields: Vec::new(),
        }
    }

    pub fn key(&self) -> String {
        if self.fields.is_empty() {
            self.base.clone()
        } else {
            format!("{}.{}", self.base, self.fields.join("."))
        }
    }

    pub fn field_path(&self, field: &str) -> AccessPath {
        let mut fields = self.fields.clone();
        fields.push(field.to_string());
        AccessPath {
            base: self.base.clone(),
            fields,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExprFact {
    /// 表达式内的 source 端点位置（未被 sanitizer 覆盖）。
    pub sources: Vec<Position>,
    /// 表达式的访问路径（裸变量与成员链；未被 sanitizer 覆盖）。
    pub paths: Vec<AccessPath>,
    /// 表达式内的调用（含嵌套）。
    pub calls: Vec<CallFact>,
    /// 整个表达式被 sanitizer 覆盖。
    pub sanitized: bool,
    /// 表达式恰为裸标识符时记录其名（用于对象组别名 `a = b`）。
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CallFact {
    pub name: String,
    /// 限定者：Java 类名 / Rust `crate::…` 路径 / 成员调用的裸标识符接收者。
    pub qualifier: Option<String>,
    pub args: Vec<ExprFact>,
    pub sanitized: bool,
}

#[derive(Debug, Clone)]
pub enum StmtFact {
    Assign {
        targets: Vec<AccessPath>,
        rhs: ExprFact,
    },
    ObjectInit {
        target: AccessPath,
        fields: Vec<(String, ExprFact)>,
    },
    Call(CallFact),
}

#[derive(Debug, Clone)]
pub struct SinkFact {
    pub span: Span,
    pub text: String,
    pub expr: ExprFact,
}

#[derive(Debug, Clone)]
pub struct FnFact {
    pub name: Option<String>,
    pub params: Vec<String>,
    /// 每个形参是否为 source（值为 source 端点位置）。
    pub param_sources: Vec<Option<Position>>,
    pub stmts: Vec<StmtFact>,
    pub sinks: Vec<SinkFact>,
    pub returns: Vec<ExprFact>,
}

#[derive(Debug, Clone)]
pub struct ImportFact {
    pub local: String,
    /// 目标模块 id 候选（相对工作区根、去扩展名；含 `/index` 与 `__init__` 变体）。
    pub modules: Vec<String>,
    pub exported: String,
}

#[derive(Debug, Clone)]
pub struct FileFacts {
    pub path: String,
    pub lang: Language,
    pub module_id: String,
    /// Java：`package` 声明 + 文件名（`a/b/C`），用于类导入解析。
    pub class_id: Option<String>,
    pub imports: Vec<ImportFact>,
    pub functions: Vec<FnFact>,
    pub degraded: bool,
}

pub fn module_id(lang: Language, path: &str) -> String {
    let normalized = path.replace('\\', "/");
    match lang {
        Language::Go => normalized
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default(),
        Language::TypeScript
        | Language::Tsx
        | Language::JavaScript
        | Language::Jsx
        | Language::Python
        | Language::Java
        | Language::Rust => strip_extension(&normalized),
    }
}

fn strip_extension(path: &str) -> String {
    match path.rsplit_once('.') {
        Some((stem, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "java" | "rs")) => {
            stem.to_string()
        }
        _ => path.to_string(),
    }
}

pub fn extract_file(
    rule: &TaintRule,
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> FileFacts {
    let root = tree.root_node();
    let sources = collect_endpoints(&rule.source_queries, root, source);
    let sinks = collect_endpoints(&rule.sink_queries, root, source);
    let sanitizers: Vec<(usize, usize)> =
        collect_endpoints(&rule.sanitizer_queries, root, source)
            .into_iter()
            .map(|e| (e.start, e.end))
            .collect();

    let prof = metrics::profile(lang);
    let mut fn_nodes: Vec<Node<'_>> = Vec::new();
    metrics::walk_all_pub(root, &mut |node| {
        if prof.functions.contains(&node.kind()) {
            fn_nodes.push(node);
        }
    });
    fn_nodes.sort_by_key(|n| n.start_byte());

    let ctx = ExtractCtx {
        lang,
        source,
        sources: &sources,
        sinks: &sinks,
        sanitizers: &sanitizers,
    };

    let functions = fn_nodes
        .iter()
        .map(|node| build_fn(&ctx, *node))
        .collect();

    FileFacts {
        path: path.to_string(),
        lang,
        module_id: module_id(lang, path),
        class_id: java_class_id(lang, root, source, path),
        imports: extract_imports(lang, root, source, path),
        functions,
        degraded,
    }
}

struct ExtractCtx<'a> {
    lang: Language,
    source: &'a str,
    sources: &'a [Endpoint],
    sinks: &'a [Endpoint],
    sanitizers: &'a [(usize, usize)],
}

impl ExtractCtx<'_> {
    fn in_sanitizer(&self, start: usize, end: usize) -> bool {
        self.sanitizers
            .iter()
            .any(|(ss, se)| start >= *ss && end <= *se)
    }
}

fn build_fn(ctx: &ExtractCtx<'_>, fn_node: Node<'_>) -> FnFact {
    let prof = metrics::profile(ctx.lang);
    let name = fn_node
        .child_by_field_name("name")
        .and_then(|n| ctx.source.get(n.start_byte()..n.end_byte()))
        .map(|s| s.to_string());
    let params = param_names(fn_node, ctx.lang, ctx.source);
    let params_node = fn_node.child_by_field_name("parameters");
    let param_sources: Vec<Option<Position>> = params
        .iter()
        .map(|name| {
            if name.is_empty() {
                return None;
            }
            let pn = params_node?;
            ctx.sources
                .iter()
                .find(|e| {
                    e.name.as_deref() == Some(name.as_str())
                        && e.start >= pn.start_byte()
                        && e.end <= pn.end_byte()
                })
                .map(|e| e.span.start)
        })
        .collect();

    let mut stmts: Vec<StmtFact> = Vec::new();
    let mut sinks: Vec<SinkFact> = Vec::new();
    let mut returns: Vec<ExprFact> = Vec::new();

    let mut visit = |node: Node<'_>| {
        let (ns, ne) = (node.start_byte(), node.end_byte());

        // sink 命中（节点种类 + 范围双等值）。先于赋值分支：
        // innerHTML 之类规则的 sink 本身就是 assignment_expression。
        if let Some(sink) = ctx
            .sinks
            .iter()
            .find(|s| s.start == ns && s.end == ne && s.kind == node.kind())
        {
            sinks.push(SinkFact {
                span: sink.span,
                text: sink.text.clone(),
                expr: expr_fact(node, ctx),
            });
        }

        // 赋值/声明。
        if let Some((_, lhs_field, rhs_field)) = prof
            .assignments
            .iter()
            .find(|(kind, _, _)| *kind == node.kind())
            .copied()
        {
            if let (Some(lhs), Some(rhs)) = (
                node.child_by_field_name(lhs_field),
                node.child_by_field_name(rhs_field),
            ) {
                let targets = access_paths_of(lhs, ctx.lang, ctx.source);
                if !targets.is_empty() {
                    if let Some(fields) = object_literal_fields(rhs, ctx) {
                        for target in targets {
                            stmts.push(StmtFact::ObjectInit { target, fields: fields.clone() });
                        }
                    } else {
                        stmts.push(StmtFact::Assign {
                            targets,
                            rhs: expr_fact(rhs, ctx),
                        });
                    }
                }
                return;
            }
        }

        // 显式 return。
        if node.kind().contains("return") {
            returns.push(expr_fact(node, ctx));
            return;
        }

        // 独立调用语句（嵌套调用由 expr_fact 覆盖）。
        if is_call_node(node, ctx.lang)
            && node
                .parent()
                .map(|p| p.kind() == "expression_statement")
                .unwrap_or(false)
        {
            if let Some(fact) = call_fact(node, ctx) {
                stmts.push(StmtFact::Call(fact));
            }
        }
    };

    for child in named_children(fn_node) {
        if prof.functions.contains(&child.kind()) {
            continue;
        }
        desc_visit(child, ctx, &mut visit);
    }

    // 尾表达式（Rust block 尾 / 箭头函数表达式体）。
    if let Some(body) = fn_node.child_by_field_name("body") {
        let body_is_block = body.kind() == "block" || body.kind() == "statement_block";
        if !body_is_block {
            returns.push(expr_fact(body, ctx));
        } else if let Some(last) = named_children(body).last() {
            if !matches!(
                last.kind(),
                "expression_statement"
                    | "let_declaration"
                    | "return_statement"
                    | "return_expression"
            ) {
                returns.push(expr_fact(*last, ctx));
            }
        }
    }

    FnFact {
        name,
        params,
        param_sources,
        stmts,
        sinks,
        returns,
    }
}

/// 深度优先访问（每节点一次，不进入嵌套函数）。
fn desc_visit<F: FnMut(Node<'_>)>(node: Node<'_>, ctx: &ExtractCtx<'_>, visit: &mut F) {
    visit(node);
    let prof = metrics::profile(ctx.lang);
    for child in named_children(node) {
        if prof.functions.contains(&child.kind()) {
            continue;
        }
        desc_visit(child, ctx, visit);
    }
}

fn is_call_node(node: Node<'_>, lang: Language) -> bool {
    match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        | Language::Go | Language::Rust => node.kind() == "call_expression",
        Language::Python => node.kind() == "call",
        Language::Java => node.kind() == "method_invocation",
    }
}

fn call_fact(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<CallFact> {
    let (callee_node, args_node) = match ctx.lang {
        Language::Java => (
            node.child_by_field_name("name")?,
            node.child_by_field_name("arguments")?,
        ),
        _ => (
            node.child_by_field_name("function")?,
            node.child_by_field_name("arguments")?,
        ),
    };
    // Java：method_invocation 的接收者（类名/实例名）作为限定者。
    let (name, qualifier) = if ctx.lang == Language::Java {
        let qualifier = node
            .child_by_field_name("object")
            .filter(|o| o.kind().contains("identifier"))
            .and_then(|o| ctx.source.get(o.start_byte()..o.end_byte()))
            .map(|s| s.to_string());
        (
            ctx.source
                .get(callee_node.start_byte()..callee_node.end_byte())
                .unwrap_or("")
                .to_string(),
            qualifier,
        )
    } else {
        callee_parts(callee_node, ctx.source)
    };
    if ctx.in_sanitizer(node.start_byte(), node.end_byte()) {
        return Some(CallFact {
            name,
            qualifier,
            args: Vec::new(),
            sanitized: true,
        });
    }
    let args = named_children(args_node)
        .into_iter()
        .filter(|a| !matches!(a.kind(), "keyword_argument" | "comment"))
        .map(|a| expr_fact(a, ctx))
        .collect();
    Some(CallFact {
        name,
        qualifier,
        args,
        sanitized: false,
    })
}

fn callee_parts(node: Node<'_>, source: &str) -> (String, Option<String>) {
    let text = |n: Node<'_>| source.get(n.start_byte()..n.end_byte()).map(|s| s.to_string());
    match node.kind() {
        k if k.contains("identifier") || k == "field_identifier" => {
            (text(node).unwrap_or_default(), None)
        }
        "member_expression" => {
            let name = node
                .child_by_field_name("property")
                .and_then(text)
                .unwrap_or_default();
            let qualifier = node
                .child_by_field_name("object")
                .filter(|o| o.kind().contains("identifier"))
                .and_then(text);
            (name, qualifier)
        }
        "field_expression" => {
            let name = node
                .child_by_field_name("field")
                .and_then(text)
                .unwrap_or_default();
            let qualifier = node
                .child_by_field_name("value")
                .filter(|o| o.kind().contains("identifier"))
                .and_then(text);
            (name, qualifier)
        }
        "attribute" => {
            let name = node
                .child_by_field_name("attribute")
                .and_then(text)
                .unwrap_or_default();
            let qualifier = node
                .child_by_field_name("object")
                .filter(|o| o.kind().contains("identifier"))
                .and_then(text);
            (name, qualifier)
        }
        "scoped_identifier" => {
            let name = node
                .child_by_field_name("name")
                .and_then(text)
                .unwrap_or_default();
            let qualifier = node.child_by_field_name("path").and_then(text);
            (name, qualifier)
        }
        "selector_expression" => {
            let name = node
                .child_by_field_name("field")
                .and_then(text)
                .unwrap_or_default();
            let qualifier = node
                .child_by_field_name("operand")
                .filter(|o| o.kind().contains("identifier"))
                .and_then(text);
            (name, qualifier)
        }
        _ => (String::new(), None),
    }
}

fn expr_fact(node: Node<'_>, ctx: &ExtractCtx<'_>) -> ExprFact {
    let (ns, ne) = (node.start_byte(), node.end_byte());
    let sanitized = ctx.in_sanitizer(ns, ne);
    let alias = if node.kind().contains("identifier") {
        Some(
            ctx.source
                .get(node.start_byte()..node.end_byte())
                .unwrap_or("")
                .to_string(),
        )
    } else {
        None
    };
    let mut fact = ExprFact {
        sources: Vec::new(),
        paths: Vec::new(),
        calls: Vec::new(),
        sanitized,
        alias,
    };
    for e in ctx.sources {
        if ns <= e.start && e.end <= ne && !ctx.in_sanitizer(e.start, e.end) {
            fact.sources.push(e.span.start);
        }
    }
    if sanitized {
        return fact;
    }
    collect_paths(node, ctx, &mut fact);
    collect_call_facts(node, ctx, &mut fact.calls);
    fact.paths.sort();
    fact.paths.dedup();
    fact
}

fn collect_call_facts(node: Node<'_>, ctx: &ExtractCtx<'_>, calls: &mut Vec<CallFact>) {
    if ctx.in_sanitizer(node.start_byte(), node.end_byte()) {
        return;
    }
    if is_call_node(node, ctx.lang) {
        if let Some(fact) = call_fact(node, ctx) {
            calls.push(fact);
        }
        return; // 实参经 args 自身递归
    }
    let prof = metrics::profile(ctx.lang);
    for child in named_children(node) {
        if prof.functions.contains(&child.kind()) {
            continue;
        }
        collect_call_facts(child, ctx, calls);
    }
}

fn collect_paths(node: Node<'_>, ctx: &ExtractCtx<'_>, fact: &mut ExprFact) {
    let (ns, ne) = (node.start_byte(), node.end_byte());
    if ctx.in_sanitizer(ns, ne) {
        return;
    }
    if is_call_node(node, ctx.lang) {
        return; // 调用实参已由 call_fact 捕获
    }
    if let Some((object, index)) = subscript_parts(node, ctx.lang) {
        if let Some(path) = access_path_of(object, ctx.lang, ctx.source) {
            fact.paths.push(path.field_path("*"));
        } else {
            collect_paths(object, ctx, fact);
        }
        collect_paths(index, ctx, fact);
        return;
    }
    if let Some(path) = access_path_of(node, ctx.lang, ctx.source) {
        fact.paths.push(path);
        return;
    }
    let prof = metrics::profile(ctx.lang);
    for child in named_children(node) {
        if prof.functions.contains(&child.kind()) {
            continue;
        }
        collect_paths(child, ctx, fact);
    }
}

fn subscript_parts<'tree>(
    node: Node<'tree>,
    lang: Language,
) -> Option<(Node<'tree>, Node<'tree>)> {
    match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        | Language::Go | Language::Rust => {
            if matches!(node.kind(), "subscript_expression" | "index_expression") {
                Some((
                    node.child_by_field_name("object")
                        .or_else(|| node.child_by_field_name("value"))?,
                    node.child_by_field_name("index")?,
                ))
            } else {
                None
            }
        }
        Language::Python => {
            if node.kind() == "subscript" {
                Some((
                    node.child_by_field_name("value")?,
                    node.child_by_field_name("subscript")?,
                ))
            } else {
                None
            }
        }
        Language::Java => {
            if node.kind() == "array_access" {
                Some((
                    node.child_by_field_name("array")?,
                    node.child_by_field_name("index")?,
                ))
            } else {
                None
            }
        }
    }
}

fn access_path_of(node: Node<'_>, lang: Language, source: &str) -> Option<AccessPath> {
    let kind = node.kind();
    let (object, field_node) = match kind {
        "member_expression" => (
            node.child_by_field_name("object")?,
            node.child_by_field_name("property")?,
        ),
        "attribute" => (
            node.child_by_field_name("object")?,
            node.child_by_field_name("attribute")?,
        ),
        "selector_expression" => (
            node.child_by_field_name("operand")?,
            node.child_by_field_name("field")?,
        ),
        "field_expression" => (
            node.child_by_field_name("value")?,
            node.child_by_field_name("field")?,
        ),
        "field_access" => (
            node.child_by_field_name("object")?,
            node.child_by_field_name("field")?,
        ),
        k if k.contains("identifier") || k == "attribute" => {
            let _ = lang;
            return Some(AccessPath::bare(
                source.get(node.start_byte()..node.end_byte())?.to_string(),
            ));
        }
        _ => return None,
    };
    let field = source
        .get(field_node.start_byte()..field_node.end_byte())?
        .to_string();
    let mut base = access_path_of(object, lang, source)?;
    base.fields.push(field);
    Some(base)
}

fn access_paths_of(node: Node<'_>, lang: Language, source: &str) -> Vec<AccessPath> {
    if let Some(path) = access_path_of(node, lang, source) {
        return vec![path];
    }
    let mut paths = Vec::new();
    collect_targets(node, lang, source, &mut paths);
    paths
}

fn collect_targets(node: Node<'_>, lang: Language, source: &str, out: &mut Vec<AccessPath>) {
    if let Some(path) = access_path_of(node, lang, source) {
        out.push(path);
        return;
    }
    for child in named_children(node) {
        collect_targets(child, lang, source, out);
    }
}

fn object_literal_fields(
    rhs: Node<'_>,
    ctx: &ExtractCtx<'_>,
) -> Option<Vec<(String, ExprFact)>> {
    let object_kind = match ctx.lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => "object",
        Language::Python => "dictionary",
        _ => return None,
    };
    if rhs.kind() != object_kind {
        return None;
    }
    let mut fields = Vec::new();
    for child in named_children(rhs) {
        match child.kind() {
            "pair" => {
                let key = child
                    .child_by_field_name("key")
                    .and_then(|k| ctx.source.get(k.start_byte()..k.end_byte()))
                    .map(|s| s.trim_matches('"').trim_matches('\'').to_string())?;
                let value = child.child_by_field_name("value")?;
                fields.push((key, expr_fact(value, ctx)));
            }
            "shorthand_property_identifier" => {
                let name = ctx
                    .source
                    .get(child.start_byte()..child.end_byte())
                    .unwrap_or("")
                    .to_string();
                fields.push((
                    name.clone(),
                    ExprFact {
                        sources: Vec::new(),
                        paths: vec![AccessPath::bare(name)],
                        calls: Vec::new(),
                        sanitized: false,
                        alias: None,
                    },
                ));
            }
            "spread_element" | "dictionary_splat" => {
                fields.push(("*".to_string(), expr_fact(child, ctx)));
            }
            _ => {}
        }
    }
    Some(fields)
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
                        names.push(first_identifier(&child, source).unwrap_or_default());
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

// ---------- 导入提取 ----------

fn extract_imports(lang: Language, root: Node<'_>, source: &str, path: &str) -> Vec<ImportFact> {
    let mut imports = Vec::new();
    match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
            metrics::walk_all_pub(root, &mut |node| {
                if node.kind() != "import_statement" {
                    return;
                }
                let Some(source_node) = named_children(node)
                    .into_iter()
                    .find(|c| c.kind() == "string")
                else {
                    return;
                };
                let spec = source
                    .get(source_node.start_byte()..source_node.end_byte())
                    .unwrap_or("")
                    .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                    .to_string();
                let Some(modules) = resolve_ts_module(path, &spec) else {
                    return;
                };
                let Some(clause) = named_children(node)
                    .into_iter()
                    .find(|c| c.kind() == "import_clause")
                else {
                    return;
                };
                for child in named_children(clause) {
                    match child.kind() {
                        "identifier" => {
                            let local = source
                                .get(child.start_byte()..child.end_byte())
                                .unwrap_or("")
                                .to_string();
                            imports.push(ImportFact {
                                local,
                                modules: modules.clone(),
                                exported: "default".into(),
                            });
                        }
                        "named_imports" => {
                            for spec_node in named_children(child) {
                                if spec_node.kind() != "import_specifier" {
                                    continue;
                                }
                                let name = spec_node
                                    .child_by_field_name("name")
                                    .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                                    .unwrap_or("")
                                    .to_string();
                                let alias = spec_node
                                    .child_by_field_name("alias")
                                    .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| name.clone());
                                if !name.is_empty() {
                                    imports.push(ImportFact {
                                        local: alias,
                                        modules: modules.clone(),
                                        exported: name,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
        Language::Python => {
            metrics::walk_all_pub(root, &mut |node| {
                if node.kind() != "import_from_statement" {
                    return;
                }
                let children = named_children(node);
                let Some(module_node) = children
                    .iter()
                    .copied()
                    .find(|c| matches!(c.kind(), "dotted_name" | "relative_import"))
                else {
                    return;
                };
                let Some(modules) = resolve_py_module(path, module_node, source) else {
                    return;
                };
                // `import` 是匿名关键字（不在 named_children 中）：跳过模块名节点后，
                // 其余具名子节点即导入项。
                for child in children {
                    if child.id() == module_node.id() {
                        continue;
                    }
                    match child.kind() {
                        "dotted_name" => {
                            let name = source
                                .get(child.start_byte()..child.end_byte())
                                .unwrap_or("")
                                .to_string();
                            if !name.is_empty() {
                                imports.push(ImportFact {
                                    local: name.clone(),
                                    modules: modules.clone(),
                                    exported: name,
                                });
                            }
                        }
                        "aliased_import" => {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                                .unwrap_or("")
                                .to_string();
                            let alias = child
                                .child_by_field_name("alias")
                                .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| name.clone());
                            if !name.is_empty() {
                                imports.push(ImportFact {
                                    local: alias,
                                    modules: modules.clone(),
                                    exported: name,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
        Language::Java => {
            metrics::walk_all_pub(root, &mut |node| {
                if node.kind() != "import_declaration" {
                    return;
                }
                let full = source
                    .get(node.start_byte()..node.end_byte())
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches(';')
                    .to_string();
                let body = full.trim_start_matches("import").trim();
                if let Some(rest) = body.strip_prefix("static ") {
                    if let Some((class_part, method)) = rest.trim().rsplit_once('.') {
                        imports.push(ImportFact {
                            local: method.to_string(),
                            modules: vec![class_part.replace('.', "/")],
                            exported: method.to_string(),
                        });
                    }
                } else if !body.is_empty() {
                    let class_id = body.replace('.', "/");
                    let local = body.rsplit('.').next().unwrap_or(body).to_string();
                    imports.push(ImportFact {
                        local: local.clone(),
                        modules: vec![class_id],
                        exported: local,
                    });
                }
            });
        }
        Language::Rust => {
            metrics::walk_all_pub(root, &mut |node| {
                if node.kind() != "use_declaration" {
                    return;
                }
                if let Some(arg) = named_children(node).last() {
                    extract_rust_use(*arg, source, &mut imports, path);
                }
            });
        }
        _ => {}
    }
    imports.sort_by(|a, b| {
        (a.local.as_str(), a.exported.as_str()).cmp(&(b.local.as_str(), b.exported.as_str()))
    });
    imports
}

fn normalize_join(dir: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn resolve_ts_module(path: &str, spec: &str) -> Option<Vec<String>> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let base = normalize_join(dir, spec);
    Some(vec![base.clone(), format!("{base}/index")])
}

fn resolve_py_module(
    path: &str,
    module_node: Node<'_>,
    source: &str,
) -> Option<Vec<String>> {
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    match module_node.kind() {
        "dotted_name" => {
            let dotted = source.get(module_node.start_byte()..module_node.end_byte())?;
            let base = dotted.replace('.', "/");
            Some(vec![base.clone(), format!("{base}/__init__")])
        }
        "relative_import" => {
            let mut prefix_len = 0usize;
            let mut dotted = String::new();
            for child in named_children(module_node) {
                match child.kind() {
                    "import_prefix" => {
                        prefix_len = source
                            .get(child.start_byte()..child.end_byte())?
                            .chars()
                            .filter(|c| *c == '.')
                            .count();
                    }
                    "dotted_name" => {
                        dotted = source
                            .get(child.start_byte()..child.end_byte())?
                            .to_string();
                    }
                    _ => {}
                }
            }
            let mut base_dir = dir.to_string();
            for _ in 1..prefix_len {
                base_dir = base_dir
                    .rsplit_once('/')
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default();
            }
            let module_path = dotted.replace('.', "/");
            let base = normalize_join(&base_dir, &module_path);
            let mut candidates = vec![base.clone(), format!("{base}/__init__")];
            if dotted.is_empty() && !base_dir.is_empty() {
                candidates.push(base_dir.clone());
                candidates.push(format!("{base_dir}/__init__"));
            }
            Some(candidates)
        }
        _ => None,
    }
}

// ---------- Java / Rust 跨文件辅助 ----------

fn java_class_id(
    lang: Language,
    root: Node<'_>,
    source: &str,
    path: &str,
) -> Option<String> {
    if lang != Language::Java {
        return None;
    }
    let file_name = path.rsplit('/').next()?;
    let stem = file_name.strip_suffix(".java")?;
    let mut package = String::new();
    metrics::walk_all_pub(root, &mut |n| {
        if n.kind() == "package_declaration" && package.is_empty() {
            for child in named_children(n) {
                if matches!(child.kind(), "scoped_identifier" | "identifier") {
                    package = source
                        .get(child.start_byte()..child.end_byte())
                        .unwrap_or("")
                        .replace('.', "/");
                }
            }
        }
    });
    if package.is_empty() {
        Some(stem.to_string())
    } else {
        Some(format!("{package}/{stem}"))
    }
}

/// Rust：文件路径 → (crate `src` 根前缀, 相对 src 的模块路径段)。
pub(crate) fn rust_src_parts(path: &str) -> Option<(String, Vec<String>)> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let src_idx = components.iter().rposition(|c| *c == "src")?;
    let src_root = components[..=src_idx].join("/");
    let rel = components[src_idx + 1..].join("/");
    if rel.is_empty() {
        return Some((src_root, Vec::new()));
    }
    let rel = strip_extension(&rel);
    let mut segments: Vec<String> = rel.split('/').map(|s| s.to_string()).collect();
    if segments.last().map(|s| s == "mod").unwrap_or(false) {
        segments.pop();
    }
    if segments.len() == 1 && (segments[0] == "lib" || segments[0] == "main") {
        segments.clear();
    }
    Some((src_root, segments))
}

fn rust_resolve_module(segments: &[String], current: &[String]) -> Vec<String> {
    let mut base: Option<Vec<String>> = None;
    let mut idx = 0usize;
    while idx < segments.len() {
        match segments[idx].as_str() {
            "crate" => {
                base = Some(Vec::new());
                idx += 1;
            }
            "super" => {
                let mut b = base.clone().unwrap_or_else(|| current.to_vec());
                b.pop();
                base = Some(b);
                idx += 1;
            }
            "self" => {
                base = Some(base.clone().unwrap_or_else(|| current.to_vec()));
                idx += 1;
            }
            _ => break,
        }
    }
    let mut resolved = base.unwrap_or_default();
    resolved.extend(segments[idx..].iter().cloned());
    resolved
}

fn rust_module_candidates(src_root: &str, module_segments: &[String]) -> Vec<String> {
    if module_segments.is_empty() {
        return vec![format!("{src_root}/lib"), format!("{src_root}/main")];
    }
    let base = format!("{src_root}/{}", module_segments.join("/"));
    vec![base.clone(), format!("{base}/mod")]
}

/// Rust 限定调用 `crate::util::run` 的限定者（模块路径）→ 模块 id 候选。
pub(crate) fn rust_qualifier_candidates(
    file_path: &str,
    qualifier: &str,
) -> Vec<String> {
    let Some((src_root, current)) = rust_src_parts(file_path) else {
        return Vec::new();
    };
    let segments: Vec<String> = qualifier.split("::").map(|s| s.to_string()).collect();
    let module = rust_resolve_module(&segments, &current);
    rust_module_candidates(&src_root, &module)
}

fn extract_rust_use(arg: Node<'_>, source: &str, imports: &mut Vec<ImportFact>, path: &str) {
    let Some((src_root, current)) = rust_src_parts(path) else {
        return;
    };
    let mut leaves: Vec<(Vec<String>, String)> = Vec::new();
    collect_rust_use_leaves(arg, source, &mut leaves);
    for (segments, local) in leaves {
        if segments.is_empty() {
            continue;
        }
        let name = segments.last().cloned().unwrap_or_default();
        let module_segments = rust_resolve_module(&segments[..segments.len() - 1], &current);
        let modules = rust_module_candidates(&src_root, &module_segments);
        imports.push(ImportFact {
            local,
            modules,
            exported: name,
        });
    }
}

/// 递归收集 use 叶子：返回（完整路径段, 本地名）。
fn collect_rust_use_leaves(
    node: Node<'_>,
    source: &str,
    out: &mut Vec<(Vec<String>, String)>,
) {
    match node.kind() {
        "use_as_clause" => {
            let Some(path_node) = node.child_by_field_name("path") else {
                return;
            };
            let alias = node
                .child_by_field_name("alias")
                .and_then(|a| source.get(a.start_byte()..a.end_byte()))
                .map(|s| s.to_string());
            let mut leaves = Vec::new();
            collect_rust_use_leaves(path_node, source, &mut leaves);
            for (segments, local) in leaves {
                let local = alias.clone().unwrap_or(local);
                out.push((segments, local));
            }
        }
        "use_list" => {
            for child in named_children(node) {
                collect_rust_use_leaves(child, source, out);
            }
        }
        "scoped_use_list" => {
            let children = named_children(node);
            let Some(prefix_node) = children.first() else {
                return;
            };
            let prefix_text = source
                .get(prefix_node.start_byte()..prefix_node.end_byte())
                .unwrap_or("")
                .to_string();
            let mut prefix: Vec<String> = prefix_text
                .split("::")
                .map(|s| s.to_string())
                .collect();
            let Some(list_node) = children.get(1) else {
                return;
            };
            let mut inner = Vec::new();
            collect_rust_use_leaves(*list_node, source, &mut inner);
            for (mut segments, local) in inner {
                let mut full = prefix.clone();
                full.append(&mut segments);
                prefix.clear();
                out.push((full, local));
            }
        }
        "use_wildcard" | "self" => {}
        "scoped_identifier" => {
            let text = source
                .get(node.start_byte()..node.end_byte())
                .unwrap_or("")
                .to_string();
            let segments: Vec<String> = text.split("::").map(|s| s.to_string()).collect();
            let local = segments.last().cloned().unwrap_or_default();
            if !local.is_empty() && local != "self" {
                out.push((segments, local));
            }
        }
        k if k.contains("identifier") => {
            let text = source
                .get(node.start_byte()..node.end_byte())
                .unwrap_or("")
                .to_string();
            if !text.is_empty() {
                out.push((vec![text.clone()], text));
            }
        }
        _ => {}
    }
}
