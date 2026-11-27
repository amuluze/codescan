//! 事实提取 IR：把（规则, 文件）的污点相关结构压缩为可跨文件分析的事实，
//! 不保留语法树。字段敏感由访问路径表达；对象字面量按字面键展开。

use super::{collect_endpoints, named_children, Endpoint};
use crate::compile::TaintRule;
use crate::language::Language;
use crate::metrics;
use crate::model::{Position, Span};
use std::collections::HashMap;
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
    /// 表达式恰为裸标识符时记录其名（用于别名 `a = b`）。
    pub alias: Option<String>,
    /// 对象字面量的字段结构（递归）；数组以 `*` 字段表示元素并集。
    pub fields: Option<Vec<(String, ExprFact)>>,
    /// 表达式恰为一次调用（用于跨函数返回对象身份回填）。
    pub call_only: bool,
    /// 条件/联合返回的分支路径（仅收集值分支，不含条件）。
    pub identity_paths: Vec<AccessPath>,
    /// 字面量表达式的真值（用于回放调用点条件）。
    pub constant: Option<bool>,
    /// 字面量文本（用于 switch case 等值 guard）。
    pub constant_text: Option<String>,
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
    Call(CallFact),
}

#[derive(Debug, Clone)]
pub struct SinkFact {
    pub span: Span,
    pub text: String,
    pub expr: ExprFact,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GuardFact {
    pub param: usize,
    pub truthy: bool,
    pub equals: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GuardedAssign {
    pub guards: Vec<GuardFact>,
    pub targets: Vec<AccessPath>,
    pub rhs: ExprFact,
}

#[derive(Debug, Clone)]
pub struct GuardedReturn {
    pub guards: Vec<GuardFact>,
    pub expr: ExprFact,
}

#[derive(Debug, Clone)]
pub struct GuardedCall {
    pub guards: Vec<GuardFact>,
    pub call: CallFact,
}

#[derive(Debug, Clone)]
pub struct GuardedSink {
    pub guards: Vec<GuardFact>,
    pub sink: SinkFact,
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
    pub guarded_assignments: Vec<GuardedAssign>,
    pub guarded_returns: Vec<GuardedReturn>,
    pub guarded_calls: Vec<GuardedCall>,
    pub guarded_sinks: Vec<GuardedSink>,
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
    /// 语言级全局赋值（当前 Go 的 package/function const/var 之外的同文件全局）。
    pub globals: Vec<StmtFact>,
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

fn endpoint_matches_imports(
    endpoint: &Endpoint,
    imported_symbols: &[String],
    aliases: &[(String, String)],
) -> bool {
    if imported_symbols.is_empty() {
        return true;
    }
    endpoint.captures.iter().any(|(_, text)| {
        let resolved = if let Some((prefix, rest)) = text.split_once('.') {
            aliases
                .iter()
                .find(|(local, _)| local == prefix)
                .map(|(_, original)| format!("{original}.{rest}"))
                .unwrap_or_else(|| text.clone())
        } else {
            aliases
                .iter()
                .find(|(local, _)| local == text)
                .map(|(_, original)| original.clone())
                .unwrap_or_else(|| text.clone())
        };
        imported_symbols.iter().any(|symbol| symbol == &resolved)
    })
}

pub fn extract_file(
    rule: &TaintRule,
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
    aliases: &[(String, String)],
) -> FileFacts {
    let root = tree.root_node();
    let sources: Vec<Endpoint> = collect_endpoints(&rule.source_queries, root, source)
        .into_iter()
        .filter(|endpoint| endpoint_matches_imports(endpoint, &rule.spec.imported_symbols, aliases))
        .collect();
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

    let empty_constants: HashMap<usize, ConstValue> = HashMap::new();
    let ctx = ExtractCtx {
        lang,
        source,
        sources: &sources,
        sinks: &sinks,
        sanitizers: &sanitizers,
        constants: &empty_constants,
    };

    let functions = fn_nodes
        .iter()
        .map(|node| build_fn(&ctx, *node))
        .collect();
    let globals = if lang == Language::Go {
        extract_globals(root, &ctx)
    } else {
        Vec::new()
    };

    FileFacts {
        path: path.to_string(),
        lang,
        module_id: module_id(lang, path),
        class_id: java_class_id(lang, root, source, path),
        imports: extract_imports(lang, root, source, path),
        functions,
        globals,
        degraded,
    }
}

fn extract_globals(root: Node<'_>, ctx: &ExtractCtx<'_>) -> Vec<StmtFact> {
    let mut globals = Vec::new();
    for declaration in named_children(root) {
        if !matches!(declaration.kind(), "var_declaration" | "const_declaration") {
            continue;
        }
        for spec in named_children(declaration) {
            if !matches!(spec.kind(), "var_spec" | "const_spec") {
                continue;
            }
            let (Some(name), Some(value)) = (
                spec.child_by_field_name("name"),
                spec.child_by_field_name("value"),
            ) else {
                continue;
            };
            let targets = access_paths_of(name, ctx.lang, ctx.source);
            if targets.is_empty() {
                continue;
            }
            globals.push(StmtFact::Assign {
                targets,
                rhs: expr_fact(value, ctx),
            });
        }
    }
    globals
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConstValue {
    truthy: Option<bool>,
    nullish: Option<bool>,
}

struct ExtractCtx<'a> {
    lang: Language,
    source: &'a str,
    sources: &'a [Endpoint],
    sinks: &'a [Endpoint],
    sanitizers: &'a [(usize, usize)],
    constants: &'a HashMap<usize, ConstValue>,
}

impl ExtractCtx<'_> {
    fn in_sanitizer(&self, start: usize, end: usize) -> bool {
        self.sanitizers
            .iter()
            .any(|(ss, se)| start >= *ss && end <= *se)
    }
}

fn build_fn(ctx: &ExtractCtx<'_>, fn_node: Node<'_>) -> FnFact {
    let constants = collect_fn_constants(ctx, fn_node);
    let fn_ctx = ExtractCtx {
        lang: ctx.lang,
        source: ctx.source,
        sources: ctx.sources,
        sinks: ctx.sinks,
        sanitizers: ctx.sanitizers,
        constants: &constants,
    };
    let ctx = &fn_ctx;
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
        // sink 命中（节点种类 + 范围双等值）。先于赋值分支：
        // innerHTML 之类规则的 sink 本身就是 assignment_expression。
        if let Some(sink) = sink_endpoint(node, ctx) {
            sinks.push(sink_fact(sink, node, ctx));
        }

        // 赋值/声明。
        if let Some((targets, rhs)) = assignment_parts(node, ctx, &prof) {
            if !targets.is_empty() {
                stmts.push(StmtFact::Assign {
                    targets,
                    rhs: expr_fact(rhs, ctx),
                });
            }
            return;
        }

        // 显式 return：只保留返回表达式，避免把 return 语句结构误当对象别名。
        if node.kind().contains("return") {
            if let Some(expr) = named_children(node).into_iter().last() {
                returns.push(expr_fact(expr, ctx));
            }
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

    let (guarded_assignments, guarded_returns, guarded_calls, guarded_sinks) =
        collect_guarded_facts(ctx, fn_node, &params);
    FnFact {
        name,
        params,
        param_sources,
        stmts,
        sinks,
        returns,
        guarded_assignments,
        guarded_returns,
        guarded_calls,
        guarded_sinks,
    }
}

fn collect_guarded_facts(
    ctx: &ExtractCtx<'_>,
    fn_node: Node<'_>,
    params: &[String],
) -> (
    Vec<GuardedAssign>,
    Vec<GuardedReturn>,
    Vec<GuardedCall>,
    Vec<GuardedSink>,
) {
    let prof = metrics::profile(ctx.lang);
    let mut facts = GuardedFacts::default();
    let Some(body) = fn_node.child_by_field_name("body") else {
        return facts.into_parts();
    };
    if body.kind() != "block" && body.kind() != "statement_block" {
        // 表达式体（箭头函数等）：整体是无 guard 的返回值。
        facts.returns.push(GuardedReturn {
            guards: Vec::new(),
            expr: expr_fact(body, ctx),
        });
        return facts.into_parts();
    }
    let walk = GuardWalk {
        ctx,
        prof: &prof,
        params,
    };
    walk.walk(
        body,
        &[],
        &mut |node: Node<'_>, guards: &[GuardFact]| {
            facts.visit_assign(node, guards, ctx, &prof)
        },
    );
    walk.walk(
        body,
        &[],
        &mut |node: Node<'_>, guards: &[GuardFact]| facts.visit_call(node, guards, ctx),
    );
    facts.into_parts()
}

/// guard 遍历收集到的带 guard 事实（按类别分开存放）。
#[derive(Default)]
struct GuardedFacts {
    assignments: Vec<GuardedAssign>,
    returns: Vec<GuardedReturn>,
    calls: Vec<GuardedCall>,
    sinks: Vec<GuardedSink>,
}

impl GuardedFacts {
    fn into_parts(
        self,
    ) -> (
        Vec<GuardedAssign>,
        Vec<GuardedReturn>,
        Vec<GuardedCall>,
        Vec<GuardedSink>,
    ) {
        (self.assignments, self.returns, self.calls, self.sinks)
    }

    /// 赋值遍历：带 guard 记录赋值与 return；这两类节点处理后不再下降。
    fn visit_assign(
        &mut self,
        node: Node<'_>,
        guards: &[GuardFact],
        ctx: &ExtractCtx<'_>,
        prof: &metrics::Profile,
    ) -> bool {
        if let Some((targets, rhs)) = assignment_parts(node, ctx, prof) {
            if !targets.is_empty() {
                self.assignments.push(GuardedAssign {
                    guards: guards.to_vec(),
                    targets,
                    rhs: expr_fact(rhs, ctx),
                });
            }
            return false;
        }
        if node.kind().contains("return") {
            if let Some(expr) = named_children(node).into_iter().last() {
                self.returns.push(GuardedReturn {
                    guards: guards.to_vec(),
                    expr: expr_fact(expr, ctx),
                });
            }
            return false;
        }
        true
    }

    /// 调用遍历：带 guard 记录 sink 与调用，随后继续下降（嵌套调用各自成条）。
    fn visit_call(&mut self, node: Node<'_>, guards: &[GuardFact], ctx: &ExtractCtx<'_>) -> bool {
        if let Some(sink) = sink_endpoint(node, ctx) {
            self.sinks.push(GuardedSink {
                guards: guards.to_vec(),
                sink: sink_fact(sink, node, ctx),
            });
        }
        if is_call_node(node, ctx.lang) {
            if let Some(call) = call_fact(node, ctx) {
                self.calls.push(GuardedCall {
                    guards: guards.to_vec(),
                    call,
                });
            }
        }
        true
    }
}

/// 带 guard 的控制流遍历：赋值遍历与调用遍历共用同一套 guard 推导。访问者在结构
/// 判断之前收到 (节点, 当前 guard)，并返回是否继续下降。
struct GuardWalk<'a> {
    ctx: &'a ExtractCtx<'a>,
    prof: &'a metrics::Profile,
    params: &'a [String],
}

impl<'a> GuardWalk<'a> {
    /// 嵌套函数的 guard 作用域独立，不进入。
    fn walk<'tree>(
        &self,
        node: Node<'tree>,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        if self.prof.functions.contains(&node.kind()) {
            return;
        }
        if !visit(node, guards) {
            return;
        }
        match node.kind() {
            kind if is_loop_kind(kind) => self.walk_loop(node, guards, visit),
            "switch_statement" => self.walk_switch(node, guards, visit),
            "if_statement" => self.walk_if(node, guards, visit),
            _ => self.walk_children(node, guards, visit),
        }
    }

    fn walk_children<'tree>(
        &self,
        node: Node<'tree>,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        for child in named_children(node) {
            self.walk(child, guards, visit);
        }
    }

    /// 循环：条件为假字面量时整体剪枝；body 之外的子节点（初始化/条件/更新）沿用当前
    /// guard；条件非字面量且可判定为形参真值时 body 追加该 guard。do-while 的条件在
    /// body 之后，全部子节点沿用当前 guard。
    fn walk_loop<'tree>(
        &self,
        node: Node<'tree>,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        if node.kind() == "do_statement" {
            self.walk_children(node, guards, visit);
            return;
        }
        let condition = node.child_by_field_name("condition");
        let literal = literal_truthiness(condition, self.ctx);
        if literal == Some(false) {
            return;
        }
        let body = node.child_by_field_name("body");
        for child in named_children(node) {
            if Some(child.id()) != body.map(|body| body.id()) {
                self.walk(child, guards, visit);
            }
        }
        let Some(body) = body else {
            return;
        };
        let mut body_guards = guards.to_vec();
        if literal.is_none() {
            if let Some((param, truthy)) = condition
                .and_then(|condition| condition_guard(condition, self.ctx, self.params))
            {
                body_guards.push(GuardFact { param, truthy, equals: None });
            }
        }
        self.walk(body, &body_guards, visit);
    }

    /// switch：switch 值是形参且 case 带值时，该 case 追加「等于 case 值」guard；
    /// default 与无法判定时沿用当前 guard。
    fn walk_switch<'tree>(
        &self,
        node: Node<'tree>,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        let switch_param = node
            .child_by_field_name("value")
            .and_then(|value| condition_guard(value, self.ctx, self.params))
            .map(|(param, _)| param);
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        for case in named_children(body) {
            if !matches!(case.kind(), "switch_case" | "switch_default") {
                continue;
            }
            let case_guards = case_guards(case, switch_param, guards, self.ctx);
            self.walk(case, &case_guards, visit);
        }
    }

    /// if：条件为字面量时只走选中分支；可判定为形参真值时 then/else 各带相反 guard；
    /// 否则两分支沿用当前 guard。
    fn walk_if<'tree>(
        &self,
        node: Node<'tree>,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        let condition = node.child_by_field_name("condition");
        let literal = literal_truthiness(condition, self.ctx);
        if literal == Some(true) || literal == Some(false) {
            let field = if literal == Some(true) {
                "consequence"
            } else {
                "alternative"
            };
            self.walk_branch(node, field, guards, visit);
            return;
        }
        let Some((param, truthy)) = condition
            .and_then(|condition| condition_guard(condition, self.ctx, self.params))
        else {
            self.walk_branch(node, "consequence", guards, visit);
            self.walk_branch(node, "alternative", guards, visit);
            return;
        };
        let then_guards = guard_with(guards, GuardFact { param, truthy, equals: None });
        self.walk_branch(node, "consequence", &then_guards, visit);
        let else_guards = guard_with(guards, GuardFact { param, truthy: !truthy, equals: None });
        self.walk_branch(node, "alternative", &else_guards, visit);
    }

    /// 遍历 `if` 语句指定字段（consequence / alternative）上的分支。
    fn walk_branch<'tree>(
        &self,
        node: Node<'tree>,
        field: &str,
        guards: &[GuardFact],
        visit: &mut dyn FnMut(Node<'tree>, &[GuardFact]) -> bool,
    ) {
        if let Some(branch) = node.child_by_field_name(field) {
            self.walk(branch, guards, visit);
        }
    }
}

/// 四种循环语句的节点种类（do-while 的条件在 body 之后）。
fn is_loop_kind(kind: &str) -> bool {
    matches!(
        kind,
        "while_statement" | "for_statement" | "for_in_statement" | "do_statement"
    )
}

/// 追加一条 guard 后的新 guard 列表。
fn guard_with(guards: &[GuardFact], guard: GuardFact) -> Vec<GuardFact> {
    let mut extended = guards.to_vec();
    extended.push(guard);
    extended
}

/// switch 分支的 guard 列表：case 带值且 switch 值是形参时追加「等于 case 值」
///（case 值文本去掉首尾引号）。
fn case_guards(
    case: Node<'_>,
    switch_param: Option<usize>,
    guards: &[GuardFact],
    ctx: &ExtractCtx<'_>,
) -> Vec<GuardFact> {
    if case.kind() != "switch_case" {
        return guards.to_vec();
    }
    let (Some(param), Some(value)) = (switch_param, case.child_by_field_name("value")) else {
        return guards.to_vec();
    };
    let Some(text) = ctx.source.get(value.start_byte()..value.end_byte()) else {
        return guards.to_vec();
    };
    let equals = text
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string();
    guard_with(guards, GuardFact {
        param,
        truthy: true,
        equals: Some(equals),
    })
}

/// 赋值/声明节点 → (lhs 目标路径, rhs 节点)。节点种类不在档位或缺少任一字段时为
/// `None`，调用方按普通节点继续处理。
fn assignment_parts<'a>(
    node: Node<'a>,
    ctx: &ExtractCtx<'_>,
    prof: &metrics::Profile,
) -> Option<(Vec<AccessPath>, Node<'a>)> {
    let (_, lhs_field, rhs_field) = prof
        .assignments
        .iter()
        .find(|(kind, _, _)| *kind == node.kind())
        .copied()?;
    let lhs = node.child_by_field_name(lhs_field)?;
    let rhs = node.child_by_field_name(rhs_field)?;
    Some((access_paths_of(lhs, ctx.lang, ctx.source), rhs))
}

/// sink 端点命中：节点种类 + 字节范围双等值。
fn sink_endpoint<'a>(node: Node<'_>, ctx: &'a ExtractCtx<'_>) -> Option<&'a Endpoint> {
    ctx.sinks.iter().find(|sink| {
        sink.start == node.start_byte()
            && sink.end == node.end_byte()
            && sink.kind == node.kind()
    })
}

/// sink 端点命中节点的 sink 事实：span/文本取端点，表达式按节点提取。
fn sink_fact(sink: &Endpoint, node: Node<'_>, ctx: &ExtractCtx<'_>) -> SinkFact {
    SinkFact {
        span: sink.span,
        text: sink.text.clone(),
        expr: expr_fact(node, ctx),
    }
}

/// 条件节点的字面量真值（条件缺失或非字面量时为 `None`）。
fn literal_truthiness(condition: Option<Node<'_>>, ctx: &ExtractCtx<'_>) -> Option<bool> {
    condition
        .and_then(|condition| literal_constant(condition, ctx))
        .and_then(|value| value.truthy)
}

fn condition_guard(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    params: &[String],
) -> Option<(usize, bool)> {
    if node.kind() == "parenthesized_expression" {
        let child = crate::taint::named_children(node).last().copied()?;
        return condition_guard(child, ctx, params);
    }
    if node.kind() == "unary_expression" {
        let operator = node
            .child_by_field_name("operator")
            .and_then(|op| ctx.source.get(op.start_byte()..op.end_byte()));
        if operator == Some("!") {
            let argument = crate::taint::named_children(node).last().copied()?;
            return condition_guard(argument, ctx, params).map(|(param, truthy)| (param, !truthy));
        }
    }
    if node.kind() == "binary_expression" {
        let operator = node
            .child_by_field_name("operator")
            .and_then(|op| ctx.source.get(op.start_byte()..op.end_byte()));
        if operator == Some("&&") {
            if let Some(left) = node.child_by_field_name("left") {
                if let Some(guard) = condition_guard(left, ctx, params) {
                    return Some(guard);
                }
            }
            if let Some(right) = node.child_by_field_name("right") {
                return condition_guard(right, ctx, params);
            }
        }
    }
    if node.kind() != "identifier" {
        return None;
    }
    let text = ctx.source.get(node.start_byte()..node.end_byte())?;
    params.iter().position(|param| param == text).map(|param| (param, true))
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
        fields: None,
        call_only: is_call_node(node, ctx.lang),
        identity_paths: identity_paths(node, ctx),
        constant: literal_constant(node, ctx).and_then(|value| value.truthy),
        constant_text: ctx
            .source
            .get(node.start_byte()..node.end_byte())
            .map(|text| text.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()),
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
    fact.fields = literal_fields(node, ctx);
    fact.paths.sort();
    fact.paths.dedup();
    fact
}

/// 对象/数组字面量结构（递归）；数组元素合并为 `*` 字段。
fn literal_fields(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<Vec<(String, ExprFact)>> {
    let object_kind = match ctx.lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => "object",
        Language::Python => "dictionary",
        _ => "",
    };
    let array_kind = match ctx.lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => "array",
        Language::Python => "list",
        _ => "",
    };
    if !object_kind.is_empty() && node.kind() == object_kind {
        let mut fields = Vec::new();
        for child in named_children(node) {
            match child.kind() {
                "pair" => {
                    let key = child
                        .child_by_field_name("key")
                        .and_then(|k| ctx.source.get(k.start_byte()..k.end_byte()))
                        .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string())
                        .unwrap_or_else(|| "*".to_string());
                    if let Some(value) = child.child_by_field_name("value") {
                        fields.push((key, expr_fact(value, ctx)));
                    }
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
                            paths: vec![AccessPath::bare(name.clone())],
                            calls: Vec::new(),
                            sanitized: false,
                            alias: Some(name),
                            fields: None,
                            call_only: false,
                            identity_paths: Vec::new(),
                            constant: None,
                            constant_text: None,
                        },
                    ));
                }
                "spread_element" | "dictionary_splat" => {
                    let value = named_children(child).first().copied();
                    if let Some(value) = value {
                        fields.push(("*".to_string(), expr_fact(value, ctx)));
                    }
                }
                _ => {}
            }
        }
        return Some(fields);
    }
    if !array_kind.is_empty() && node.kind() == array_kind {
        let elements: Vec<ExprFact> = named_children(node)
            .into_iter()
            .map(|e| expr_fact(e, ctx))
            .collect();
        if elements.is_empty() {
            return Some(Vec::new());
        }
        return Some(vec![("*".to_string(), merge_expr_facts(&elements))]);
    }
    None
}

fn collect_fn_constants(
    ctx: &ExtractCtx<'_>,
    fn_node: Node<'_>,
) -> HashMap<usize, ConstValue> {
    let prof = metrics::profile(ctx.lang);
    let Some(body) = fn_node.child_by_field_name("body") else {
        return HashMap::new();
    };
    let mut current = HashMap::new();
    let mut snapshots = HashMap::new();
    collect_constants_in(body, ctx, &prof, &mut current, &mut snapshots, true);
    snapshots
}

fn collect_constants_in(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    current: &mut HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
    is_root: bool,
) {
    // 标签与 break/continue 目标都是函数局部的；嵌套函数不参与外层常量/标签分析。
    if !is_root && prof.functions.contains(&node.kind()) {
        return;
    }
    match node.kind() {
        "if_statement" => collect_if_constants(node, ctx, prof, current, snapshots),
        "labeled_statement" => {
            let flow = collect_flow(node, ctx, prof, current.clone(), snapshots);
            *current = merge_flow_normal(&flow);
        }
        "while_statement" | "for_statement" | "for_in_statement" | "do_statement" => {
            let flow = collect_loop_flow(node, ctx, prof, current.clone(), snapshots);
            *current = merge_flow_normal(&flow);
        }
        "switch_statement" => {
            let flow = collect_switch_flow(node, ctx, prof, current.clone(), snapshots);
            *current = merge_flow_normal(&flow);
        }
        "try_statement" => collect_try_constants(node, ctx, prof, current, snapshots),
        _ => {
            for child in crate::taint::named_children(node) {
                collect_constants_in(child, ctx, prof, current, snapshots, false);
            }
            snapshot_identifier(node, ctx, current, snapshots);
            record_assignment(node, ctx, current);
        }
    }
}

/// if 语句的常量传播：条件先按当前状态传播；可判定时只走选中分支，否则两分支各自
/// 传播后合并。
fn collect_if_constants(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    current: &mut HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) {
    let condition = node.child_by_field_name("condition");
    if let Some(condition) = condition {
        collect_constants_in(condition, ctx, prof, current, snapshots, false);
    }
    let known = condition.and_then(|condition| condition_truthiness(condition, ctx, current));
    let selected = match known {
        Some(true) => node.child_by_field_name("consequence"),
        Some(false) => node.child_by_field_name("alternative"),
        None => {
            let mut consequent = current.clone();
            if let Some(consequence) = node.child_by_field_name("consequence") {
                collect_constants_in(consequence, ctx, prof, &mut consequent, snapshots, false);
            }
            let mut alternative = current.clone();
            if let Some(alternative_node) = node.child_by_field_name("alternative") {
                collect_constants_in(alternative_node, ctx, prof, &mut alternative, snapshots, false);
            }
            *current = merge_constants(&consequent, &alternative);
            return;
        }
    };
    if let Some(branch) = selected {
        let mut selected_state = current.clone();
        collect_constants_in(branch, ctx, prof, &mut selected_state, snapshots, false);
        *current = selected_state;
    }
}

/// try：try 体与 catch 各自从进入前的状态出发，二者合并后再走 finally。
fn collect_try_constants(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    current: &mut HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) {
    let mut try_state = current.clone();
    if let Some(body) = node.child_by_field_name("body") {
        collect_constants_in(body, ctx, prof, &mut try_state, snapshots, false);
    }
    let mut merged = try_state;
    if let Some(handler) = node.child_by_field_name("handler") {
        let mut catch_state = current.clone();
        collect_constants_in(handler, ctx, prof, &mut catch_state, snapshots, false);
        merged = merge_constants(&merged, &catch_state);
    }
    if let Some(finalizer) = node.child_by_field_name("finalizer") {
        collect_constants_in(finalizer, ctx, prof, &mut merged, snapshots, false);
    }
    *current = merged;
}

/// 标识符读取：把当前已知的常量值按节点字节位置记入快照。
fn snapshot_identifier(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    current: &HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) {
    if node.kind() != "identifier" {
        return;
    }
    let Some(text) = ctx.source.get(node.start_byte()..node.end_byte()) else {
        return;
    };
    if let Some(value) = current.get(text) {
        snapshots.insert(node.start_byte(), *value);
    }
}

/// 赋值/声明的常量更新：右值为字面量则记录，否则清除该名字的已知值。
fn record_assignment(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    current: &mut HashMap<String, ConstValue>,
) {
    let name_node = match node.kind() {
        "variable_declarator" | "var_spec" | "const_spec" | "let_spec" => {
            node.child_by_field_name("name")
                .filter(|name| name.kind() == "identifier")
                .map(|name| (name, node.child_by_field_name("value")))
        }
        "assignment_expression" | "assignment" => node
            .child_by_field_name("left")
            .filter(|left| left.kind() == "identifier")
            .map(|left| (left, node.child_by_field_name("right"))),
        _ => None,
    };
    let Some((name, value)) = name_node else {
        return;
    };
    let name = ctx
        .source
        .get(name.start_byte()..name.end_byte())
        .unwrap_or("")
        .to_string();
    if let Some(value) = value.and_then(|value| literal_constant(value, ctx)) {
        current.insert(name, value);
    } else {
        current.remove(&name);
    }
}

fn merge_all_constants(states: Vec<HashMap<String, ConstValue>>) -> HashMap<String, ConstValue> {
    let mut iter = states.into_iter();
    let mut merged = iter.next().unwrap_or_default();
    for state in iter {
        merged = merge_constants(&merged, &state);
    }
    merged
}

#[derive(Default)]
struct ConstFlow {
    normal: Vec<HashMap<String, ConstValue>>,
    breaks: Vec<(Option<String>, HashMap<String, ConstValue>)>,
    continues: Vec<(Option<String>, HashMap<String, ConstValue>)>,
}

impl ConstFlow {
    /// 单一正常出口（无 break/continue）。
    fn just(state: HashMap<String, ConstValue>) -> Self {
        ConstFlow {
            normal: vec![state],
            ..Default::default()
        }
    }

    /// 顺序并入另一段流的全部出口（不合并）。
    fn extend_from(&mut self, other: ConstFlow) {
        self.normal.extend(other.normal);
        self.breaks.extend(other.breaks);
        self.continues.extend(other.continues);
    }

    /// 正常出口合并为一个；break/continue 出口不变。
    fn with_merged_normal(self) -> Self {
        ConstFlow {
            normal: vec![merge_all_constants(self.normal)],
            breaks: self.breaks,
            continues: self.continues,
        }
    }
}

fn merge_flow_normal(flow: &ConstFlow) -> HashMap<String, ConstValue> {
    merge_all_constants(flow.normal.clone())
}

fn outcome_label(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<String> {
    node.child_by_field_name("label")
        .and_then(|label| ctx.source.get(label.start_byte()..label.end_byte()))
        .map(|label| label.to_string())
}

fn collect_loop_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    mut state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    if node.kind() == "do_statement" {
        return collect_do_flow(node, ctx, prof, state, snapshots);
    }
    let body = node.child_by_field_name("body");
    for child in crate::taint::named_children(node) {
        if Some(child.id()) != body.map(|body| body.id()) {
            collect_constants_in(child, ctx, prof, &mut state, snapshots, false);
        }
    }
    let mut states = vec![state.clone()];
    let mut breaks = Vec::new();
    let mut continues = Vec::new();
    if let Some(body) = body {
        let flow = collect_flow(body, ctx, prof, state, snapshots);
        absorb_flow(flow, &mut states, &mut breaks, &mut continues);
    }
    ConstFlow {
        normal: vec![merge_all_constants(states)],
        breaks,
        continues,
    }
}

/// do-while：body 至少执行一次，其出口直接作为循环出口；随后的条件再传播常量。
fn collect_do_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    let mut states = Vec::new();
    let mut breaks = Vec::new();
    let mut continues = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let flow = collect_flow(body, ctx, prof, state.clone(), snapshots);
        absorb_flow(flow, &mut states, &mut breaks, &mut continues);
    } else {
        states.push(state.clone());
    }
    let mut body_state = merge_all_constants(states);
    if let Some(condition) = node.child_by_field_name("condition") {
        collect_constants_in(condition, ctx, prof, &mut body_state, snapshots, false);
    }
    ConstFlow {
        normal: vec![body_state],
        breaks,
        continues,
    }
}

/// 语句流的出口并入循环迭代：无标签 break/continue 结束本次迭代（并入 `states`），
/// 带标签者向外透传。
fn absorb_flow(
    flow: ConstFlow,
    states: &mut Vec<HashMap<String, ConstValue>>,
    breaks: &mut Vec<(Option<String>, HashMap<String, ConstValue>)>,
    continues: &mut Vec<(Option<String>, HashMap<String, ConstValue>)>,
) {
    states.extend(flow.normal);
    for (label, state) in flow.breaks {
        if label.is_none() {
            states.push(state);
        } else {
            breaks.push((label, state));
        }
    }
    for (label, state) in flow.continues {
        if label.is_none() {
            states.push(state);
        } else {
            continues.push((label, state));
        }
    }
}

fn collect_switch_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    mut state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    if let Some(value) = node.child_by_field_name("value") {
        collect_constants_in(value, ctx, prof, &mut state, snapshots, false);
    }
    let mut cases = Vec::new();
    let mut has_default = false;
    if let Some(body) = node.child_by_field_name("body") {
        for case in crate::taint::named_children(body) {
            if !matches!(case.kind(), "switch_case" | "switch_default") {
                continue;
            }
            if case.kind() == "switch_default" {
                has_default = true;
            }
            cases.push(case);
        }
    }
    let mut normal = Vec::new();
    let mut breaks = Vec::new();
    let mut continues = Vec::new();
    for start in 0..cases.len() {
        let mut current = state.clone();
        for (idx, case) in cases.iter().enumerate().skip(start) {
            let statements: Vec<Node<'_>> = {
                let mut cursor = case.walk();
                case.children_by_field_name("body", &mut cursor).collect()
            };
            let flow = collect_statement_list_flow(&statements, ctx, prof, current.clone(), snapshots);
            for (label, state) in flow.breaks {
                if label.is_none() {
                    normal.push(state);
                } else {
                    breaks.push((label, state));
                }
            }
            continues.extend(flow.continues);
            let next = merge_all_constants(flow.normal);
            if next.is_empty() {
                break;
            }
            current = next;
            if idx + 1 == cases.len() {
                normal.push(current.clone());
            }
        }
    }
    if !has_default {
        normal.push(state);
    }
    ConstFlow {
        normal: vec![merge_all_constants(normal)],
        breaks,
        continues,
    }
}

fn collect_statement_list_flow(
    statements: &[Node<'_>],
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    let mut normal = vec![state];
    let mut breaks = Vec::new();
    let mut continues = Vec::new();
    for child in statements {
        let mut next = Vec::new();
        for branch in normal.drain(..) {
            let flow = collect_flow(*child, ctx, prof, branch, snapshots);
            next.extend(flow.normal);
            breaks.extend(flow.breaks);
            continues.extend(flow.continues);
        }
        normal = next;
        if normal.is_empty() {
            break;
        }
    }
    ConstFlow {
        normal,
        breaks,
        continues,
    }
}

fn collect_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    match node.kind() {
        "statement_block" => {
            let statements = crate::taint::named_children(node);
            collect_statement_list_flow(&statements, ctx, prof, state, snapshots)
        }
        "break_statement" => ConstFlow {
            breaks: vec![(outcome_label(node, ctx), state)],
            ..Default::default()
        },
        "continue_statement" => ConstFlow {
            continues: vec![(outcome_label(node, ctx), state)],
            ..Default::default()
        },
        "labeled_statement" => collect_labeled_flow(node, ctx, prof, state, snapshots),
        "else_clause" => {
            if let Some(child) = crate::taint::named_children(node).last().copied() {
                collect_flow(child, ctx, prof, state, snapshots)
            } else {
                ConstFlow::just(state)
            }
        }
        "if_statement" => collect_if_flow(node, ctx, prof, state, snapshots),
        "while_statement" | "for_statement" | "for_in_statement" | "do_statement" => {
            collect_loop_flow(node, ctx, prof, state, snapshots)
        }
        "switch_statement" => collect_switch_flow(node, ctx, prof, state, snapshots),
        _ => {
            let mut next = state;
            collect_constants_in(node, ctx, prof, &mut next, snapshots, false);
            ConstFlow::just(next)
        }
    }
}

/// 带标签语句的流：目标为其它标签的 break/continue 向外透传；匹配本标签的 break 在
/// 标签语句之后正常继续；匹配本标签的 continue 在循环上回灌下一轮循环体，在非循环上
/// 按正常出口处理。缺少标签或 body 时状态原样流出。
fn collect_labeled_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    let (Some(label), Some(body)) =
        (outcome_label(node, ctx), node.child_by_field_name("body"))
    else {
        return ConstFlow::just(state);
    };
    let mut flow = LabelFlow {
        label,
        normal: Vec::new(),
        breaks: Vec::new(),
        continues: Vec::new(),
    };
    let inner = collect_flow(body, ctx, prof, state, snapshots);
    flow.normal.extend(inner.normal);
    flow.route_breaks(inner.breaks);
    let matched_continues = flow.route_continues(inner.continues);
    if is_loop_kind(body.kind()) {
        replay_continues(body, ctx, prof, &mut flow, matched_continues, snapshots);
    } else {
        flow.normal.extend(matched_continues);
    }
    flow.into_flow()
}

/// 循环标签上匹配的 continue：合并状态后重新进入循环体（至多 8 轮）。期间匹配本标签的
/// break 归入正常出口，新的匹配 continue 继续回灌；未耗尽的回灌状态并入正常出口。
fn replay_continues(
    body: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    flow: &mut LabelFlow,
    mut pending: Vec<HashMap<String, ConstValue>>,
    snapshots: &mut HashMap<usize, ConstValue>,
) {
    let loop_body = body.child_by_field_name("body");
    let mut iterations = 0;
    while !pending.is_empty() && iterations < 8 {
        let entry = merge_all_constants(pending);
        pending = Vec::new();
        if let Some(loop_body) = loop_body {
            let next = collect_flow(loop_body, ctx, prof, entry, snapshots);
            flow.normal.extend(next.normal);
            flow.route_breaks(next.breaks);
            pending = flow.route_continues(next.continues);
        }
        iterations += 1;
    }
    flow.normal.extend(pending);
}

/// 带标签语句的出口累积：按目标标签归类 break/continue。
struct LabelFlow {
    label: String,
    normal: Vec<HashMap<String, ConstValue>>,
    breaks: Vec<(Option<String>, HashMap<String, ConstValue>)>,
    continues: Vec<(Option<String>, HashMap<String, ConstValue>)>,
}

impl LabelFlow {
    fn targets_label(&self, label: &Option<String>) -> bool {
        label.as_deref() == Some(self.label.as_str())
    }

    /// break：匹配本标签者并入正常出口，其余向外透传。
    fn route_breaks(&mut self, breaks: Vec<(Option<String>, HashMap<String, ConstValue>)>) {
        for (label, state) in breaks {
            if self.targets_label(&label) {
                self.normal.push(state);
            } else {
                self.breaks.push((label, state));
            }
        }
    }

    /// continue：其余向外透传，匹配本标签者返回给调用方（回灌循环体或并入正常出口）。
    fn route_continues(
        &mut self,
        continues: Vec<(Option<String>, HashMap<String, ConstValue>)>,
    ) -> Vec<HashMap<String, ConstValue>> {
        let mut matched = Vec::new();
        for (label, state) in continues {
            if self.targets_label(&label) {
                matched.push(state);
            } else {
                self.continues.push((label, state));
            }
        }
        matched
    }

    fn into_flow(self) -> ConstFlow {
        ConstFlow {
            normal: vec![merge_all_constants(self.normal)],
            breaks: self.breaks,
            continues: self.continues,
        }
    }
}

/// if 语句的流：条件先传播常量；可判定时只走选中分支，否则两分支都走后合并。
fn collect_if_flow(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    prof: &crate::metrics::Profile,
    state: HashMap<String, ConstValue>,
    snapshots: &mut HashMap<usize, ConstValue>,
) -> ConstFlow {
    let mut condition_state = state;
    let condition = node.child_by_field_name("condition");
    if let Some(condition) = condition {
        collect_constants_in(condition, ctx, prof, &mut condition_state, snapshots, false);
    }
    let known =
        condition.and_then(|condition| condition_truthiness(condition, ctx, &condition_state));
    let consequence = node
        .child_by_field_name("consequence")
        .map(|consequence| {
            collect_flow(consequence, ctx, prof, condition_state.clone(), snapshots)
        });
    if known == Some(true) {
        return consequence.unwrap_or_default();
    }
    if known == Some(false) {
        if let Some(alternative) = node.child_by_field_name("alternative") {
            return collect_flow(alternative, ctx, prof, condition_state, snapshots);
        }
        return ConstFlow::just(condition_state);
    }
    // 两分支都可能执行：各自从条件后的状态出发，出口合并。
    let mut merged = ConstFlow::default();
    merged.extend_from(consequence.unwrap_or_else(|| ConstFlow::just(condition_state.clone())));
    if let Some(alternative) = node.child_by_field_name("alternative") {
        merged.extend_from(collect_flow(alternative, ctx, prof, condition_state, snapshots));
    } else {
        merged.normal.push(condition_state);
    }
    merged.with_merged_normal()
}

fn merge_constants(
    left: &HashMap<String, ConstValue>,
    right: &HashMap<String, ConstValue>,
) -> HashMap<String, ConstValue> {
    left.iter()
        .filter(|(name, value)| right.get(*name) == Some(*value))
        .map(|(name, value)| (name.clone(), *value))
        .collect()
}

fn condition_truthiness(
    node: Node<'_>,
    ctx: &ExtractCtx<'_>,
    values: &HashMap<String, ConstValue>,
) -> Option<bool> {
    if node.kind() == "parenthesized_expression" {
        let child = crate::taint::named_children(node).last().copied()?;
        return condition_truthiness(child, ctx, values);
    }
    if node.kind() == "unary_expression" {
        let operator = node
            .child_by_field_name("operator")
            .and_then(|op| ctx.source.get(op.start_byte()..op.end_byte()));
        if operator == Some("!") {
            let argument = crate::taint::named_children(node).last().copied()?;
            return condition_truthiness(argument, ctx, values).map(|value| !value);
        }
    }
    if node.kind() == "identifier" {
        let text = ctx.source.get(node.start_byte()..node.end_byte())?;
        return values.get(text.trim()).and_then(|value| value.truthy);
    }
    literal_constant(node, ctx).and_then(|value| value.truthy)
}

fn literal_constant(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<ConstValue> {
    if node.kind() == "parenthesized_expression" {
        let child = crate::taint::named_children(node).last().copied()?;
        return literal_constant(child, ctx);
    }
    if node.kind() == "unary_expression" {
        let operator = node
            .child_by_field_name("operator")
            .and_then(|op| ctx.source.get(op.start_byte()..op.end_byte()));
        if operator == Some("!") {
            let argument = crate::taint::named_children(node).last().copied()?;
            let value = literal_constant(argument, ctx)?.truthy?;
            return Some(ConstValue {
                truthy: Some(!value),
                nullish: Some(false),
            });
        }
    }
    let text = ctx.source.get(node.start_byte()..node.end_byte())?.trim();
    match text {
        "true" => Some(ConstValue { truthy: Some(true), nullish: Some(false) }),
        "false" | "NaN" => Some(ConstValue { truthy: Some(false), nullish: Some(false) }),
        "null" | "undefined" => Some(ConstValue { truthy: Some(false), nullish: Some(true) }),
        _ => match node.kind() {
            "number" | "integer" | "float" => Some(ConstValue {
                truthy: Some(text.parse::<f64>().ok()? != 0.0),
                nullish: Some(false),
            }),
            "string" | "template_string" => Some(ConstValue {
                truthy: Some(!text.trim_matches(|c| c == '"' || c == '\'').is_empty()),
                nullish: Some(false),
            }),
            "object" | "array" | "dictionary" | "list" => Some(ConstValue {
                truthy: Some(true),
                nullish: Some(false),
            }),
            _ => None,
        },
    }
}

fn identity_paths(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Vec<AccessPath> {
    let mut paths = Vec::new();
    match node.kind() {
        "ternary_expression" | "conditional_expression" => {
            let condition = node
                .child_by_field_name("condition")
                .and_then(|condition| constant_truthiness(condition, ctx));
            let fields: &[&str] = match condition {
                Some(true) => &["consequence"],
                Some(false) => &["alternative"],
                None => &["consequence", "alternative"],
            };
            for field in fields {
                if let Some(branch) = node.child_by_field_name(*field) {
                    collect_identity_branch(branch, ctx, &mut paths);
                }
            }
        }
        "binary_expression"
            if matches!(
                ctx.lang,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            ) =>
        {
            let operator = node
                .child_by_field_name("operator")
                .and_then(|operator| ctx.source.get(operator.start_byte()..operator.end_byte()));
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            let selected: &[Option<Node<'_>>] = match operator {
                Some("??") => match left.and_then(|left| constant_nullish(left, ctx)) {
                    Some(true) => &[right],
                    Some(false) => &[left],
                    None => &[left, right],
                },
                Some("||") => match left.and_then(|left| constant_truthiness(left, ctx)) {
                    Some(true) => &[left],
                    Some(false) => &[right],
                    None => &[left, right],
                },
                _ => &[],
            };
            for branch in selected.iter().flatten() {
                collect_identity_branch(*branch, ctx, &mut paths);
            }
        }
        "parenthesized_expression" => {
            if let Some(child) = crate::taint::named_children(node).last() {
                paths.extend(identity_paths(*child, ctx));
            }
        }
        _ => {}
    }
    paths.sort();
    paths.dedup();
    paths
}

fn constant_truthiness(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<bool> {
    if let Some(value) = ctx.constants.get(&node.start_byte()) {
        return value.truthy;
    }
    if node.kind() == "parenthesized_expression" {
        let child = crate::taint::named_children(node).last().copied()?;
        return constant_truthiness(child, ctx);
    }
    if node.kind() == "unary_expression" {
        let operator = node
            .child_by_field_name("operator")
            .and_then(|op| ctx.source.get(op.start_byte()..op.end_byte()));
        if operator == Some("!") {
            let argument = crate::taint::named_children(node).last().copied()?;
            return constant_truthiness(argument, ctx).map(|value| !value);
        }
    }
    let text = ctx
        .source
        .get(node.start_byte()..node.end_byte())?
        .trim();
    match text {
        "true" => Some(true),
        "false" | "null" | "undefined" | "NaN" => Some(false),
        _ => match node.kind() {
            "number" | "integer" | "float" => text
                .parse::<f64>()
                .ok()
                .map(|value| value != 0.0),
            "string" | "template_string" => Some(!text.trim_matches(|c| c == '"' || c == '\'').is_empty()),
            "object" | "array" | "dictionary" | "list" => Some(true),
            _ => None,
        },
    }
}

fn constant_nullish(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<bool> {
    if let Some(value) = ctx.constants.get(&node.start_byte()) {
        return value.nullish;
    }
    if node.kind() == "parenthesized_expression" {
        let child = crate::taint::named_children(node).last().copied()?;
        return constant_nullish(child, ctx);
    }
    let text = ctx
        .source
        .get(node.start_byte()..node.end_byte())?
        .trim();
    match text {
        "null" | "undefined" => Some(true),
        "true" | "false" | "NaN" => Some(false),
        _ => match node.kind() {
            "number" | "integer" | "float" | "string" | "template_string" | "object" | "array"
            | "dictionary" | "list" => Some(false),
            _ => None,
        },
    }
}

fn collect_identity_branch(node: Node<'_>, ctx: &ExtractCtx<'_>, paths: &mut Vec<AccessPath>) {
    if let Some(path) = access_path_of(node, ctx.lang, ctx.source) {
        paths.push(path);
        return;
    }
    let nested = identity_paths(node, ctx);
    if !nested.is_empty() {
        paths.extend(nested);
    }
}

fn merge_expr_facts(items: &[ExprFact]) -> ExprFact {
    let mut merged = ExprFact {
        sanitized: items.iter().all(|e| e.sanitized),
        ..Default::default()
    };
    for item in items {
        merged.sources.extend(item.sources.iter().copied());
        merged.paths.extend(item.paths.iter().cloned());
        merged.calls.extend(item.calls.iter().cloned());
    }
    merged.paths.sort();
    merged.paths.dedup();
    merged
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
        "subscript_expression" | "index_expression" => {
            let object = node
                .child_by_field_name("object")
                .or_else(|| node.child_by_field_name("value"))?;
            let mut base = access_path_of(object, lang, source)?;
            base.fields.push("*".to_string());
            return Some(base);
        }
        "subscript" => {
            let object = node.child_by_field_name("value")?;
            let mut base = access_path_of(object, lang, source)?;
            base.fields.push("*".to_string());
            return Some(base);
        }
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
            extract_js_imports(root, source, path, &mut imports);
        }
        Language::Python => extract_python_imports(root, source, path, &mut imports),
        Language::Java => extract_java_imports(root, source, &mut imports),
        Language::Rust => extract_rust_imports(root, source, path, &mut imports),
        _ => {}
    }
    imports.sort_by(|a, b| {
        (a.local.as_str(), a.exported.as_str()).cmp(&(b.local.as_str(), b.exported.as_str()))
    });
    imports
}

/// TS/JS：`import defaultExport, { a, b as c } from "./mod"`。仅解析相对路径导入。
fn extract_js_imports(root: Node<'_>, source: &str, path: &str, imports: &mut Vec<ImportFact>) {
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
                "named_imports" => extract_named_imports(child, source, &modules, imports),
                _ => {}
            }
        }
    });
}

/// `import { a, b as c } ...`：每个 import_specifier 一条导入。
fn extract_named_imports(
    node: Node<'_>,
    source: &str,
    modules: &[String],
    imports: &mut Vec<ImportFact>,
) {
    for spec_node in named_children(node) {
        if spec_node.kind() != "import_specifier" {
            continue;
        }
        aliased_import(spec_node, "name", source, modules, imports);
    }
}

/// 带可选别名的导入项（TS import_specifier / Python aliased_import）：`alias` 字段缺失时
/// 本地名即导出名；导出名为空时忽略。
fn aliased_import(
    node: Node<'_>,
    name_field: &str,
    source: &str,
    modules: &[String],
    imports: &mut Vec<ImportFact>,
) {
    let Some(name) = node
        .child_by_field_name(name_field)
        .and_then(|n| source.get(n.start_byte()..n.end_byte()))
    else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let name = name.to_string();
    let alias = node
        .child_by_field_name("alias")
        .and_then(|n| source.get(n.start_byte()..n.end_byte()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| name.clone());
    imports.push(ImportFact {
        local: alias,
        modules: modules.to_vec(),
        exported: name,
    });
}

/// Python：`from mod import a, b as c`（含相对导入）。
fn extract_python_imports(root: Node<'_>, source: &str, path: &str, imports: &mut Vec<ImportFact>) {
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
                "aliased_import" => aliased_import(child, "name", source, &modules, imports),
                _ => {}
            }
        }
    });
}

/// Java：`import a.b.C` 与 `import static a.b.C.m`。
fn extract_java_imports(root: Node<'_>, source: &str, imports: &mut Vec<ImportFact>) {
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

/// Rust：`use a::b::{c, d as e}`（按 crate `src/` 模块树映射）。
fn extract_rust_imports(root: Node<'_>, source: &str, path: &str, imports: &mut Vec<ImportFact>) {
    metrics::walk_all_pub(root, &mut |node| {
        if node.kind() != "use_declaration" {
            return;
        }
        if let Some(arg) = named_children(node).last() {
            extract_rust_use(*arg, source, imports, path);
        }
    });
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
