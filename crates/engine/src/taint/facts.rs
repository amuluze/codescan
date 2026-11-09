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
        | Language::Python => strip_extension(&normalized),
        _ => normalized,
    }
}

fn strip_extension(path: &str) -> String {
    match path.rsplit_once('.') {
        Some((stem, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py")) => stem.to_string(),
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
    // 无法解析被调名时仍保留调用事实（实参求值/流经调用点）。
    let name = callee_name(callee_node, ctx.source).unwrap_or_default();
    if ctx.in_sanitizer(node.start_byte(), node.end_byte()) {
        return Some(CallFact {
            name,
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
        args,
        sanitized: false,
    })
}

fn callee_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        k if k.contains("identifier") || k == "field_identifier" => {
            source.get(node.start_byte()..node.end_byte()).map(|s| s.to_string())
        }
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "attribute" => node
            .child_by_field_name("attribute")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        "selector_expression" => node
            .child_by_field_name("field")
            .and_then(|p| source.get(p.start_byte()..p.end_byte()))
            .map(|s| s.to_string()),
        _ => None,
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
        Language::Go => {
            // Go 同包跨文件由项目分析按目录处理，无需导入表。
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
