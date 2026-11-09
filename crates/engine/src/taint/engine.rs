//! 项目级污点分析：模块解析（同文件/显式导入/Go 同包）、布尔摘要不动点、
//! 字段敏感访问路径与对象组别名近似。语义见 Domain §4a/4b。

use super::facts::{AccessPath, CallFact, ExprFact, FileFacts, FnFact, StmtFact};
use crate::compile::TaintRule;
use crate::model::{Finding, Position};
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_CALL_DEPTH: usize = 6;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Summary {
    /// 形参索引 → 可达 sink。
    sink_from_param: HashSet<usize>,
    /// 形参索引 → 返回值携带其污点。
    ret_from_param: HashSet<usize>,
    /// 返回值的内部 source 污点位置（与形参无关）。
    ret_internal: Option<Position>,
}

#[derive(Debug, Clone, Copy)]
struct GlobalFn {
    file_idx: usize,
    func_idx: usize,
}

struct Project<'a> {
    rule: &'a TaintRule,
    files: &'a [FileFacts],
    globals: Vec<GlobalFn>,
    /// 每文件：本地名 → 全局函数 id（本地 + 显式导入 + Go 同包）。
    resolution: Vec<HashMap<String, Vec<usize>>>,
    summaries: Vec<Summary>,
}

impl<'a> Project<'a> {
    fn fn_fact(&self, gid: usize) -> &FnFact {
        let g = self.globals[gid];
        &self.files[g.file_idx].functions[g.func_idx]
    }

    fn resolve(&self, file_idx: usize, name: &str) -> Vec<usize> {
        self.resolution[file_idx]
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    fn analyze(&mut self) -> Vec<Finding> {
        self.compute_summaries();
        let mut merged: BTreeMap<(String, usize, usize, usize, usize), Finding> = BTreeMap::new();
        for gid in 0..self.globals.len() {
            let mut state = TaintState::default();
            let f = self.fn_fact(gid);
            for (idx, pos) in f.param_sources.iter().enumerate() {
                if let (Some(pos), Some(name)) = (pos, f.params.get(idx)) {
                    if !name.is_empty() {
                        state.set_whole(name, *pos);
                    }
                }
            }
            let file_idx = self.globals[gid].file_idx;
            let mut stack = Vec::new();
            self.collect_fn(gid, file_idx, &mut state, 0, &mut stack, &mut merged);
        }
        merged.into_values().collect()
    }

    fn compute_summaries(&mut self) {
        self.summaries = vec![Summary::default(); self.globals.len()];
        let total_params: usize = (0..self.globals.len())
            .map(|gid| self.fn_fact(gid).params.len())
            .sum();
        let max_iterations = total_params + self.globals.len() + 2;
        let mut iterations = 0;
        loop {
            let old = self.summaries.clone();
            let mut next = old.clone();
            for (gid, next_summary) in next.iter_mut().enumerate() {
                // 返回值内部污点。
                if next_summary.ret_internal.is_none() {
                    let f = self.fn_fact(gid);
                    let state = TaintState::default();
                    if let Some(flow) = self.eval_returns(f, &state, &old) {
                        next_summary.ret_internal = Some(flow);
                    }
                }
                let names = {
                    let f = self.fn_fact(gid);
                    f.params.clone()
                };
                for (p, name) in names.iter().enumerate() {
                    let name = name.clone();
                    if name.is_empty() {
                        continue;
                    }
                    let needs_sink = !next_summary.sink_from_param.contains(&p);
                    let needs_ret = !next_summary.ret_from_param.contains(&p);
                    if !needs_sink && !needs_ret {
                        continue;
                    }
                    let mut state = TaintState::default();
                    state.set_whole(&name, Position { line: 1, column: 1 });
                    let f = self.fn_fact(gid);
                    self.apply_stmts(f, &mut state, &old);
                    if needs_sink && self.any_sink_flow(f, &state, &old) {
                        next_summary.sink_from_param.insert(p);
                    }
                    if needs_ret && self.eval_returns(f, &state, &old).is_some() {
                        next_summary.ret_from_param.insert(p);
                    }
                }
            }
            let changed = next != old;
            self.summaries = next;
            iterations += 1;
            if !changed {
                break;
            }
            assert!(
                iterations <= max_iterations,
                "污点摘要未在 {max_iterations} 轮内收敛"
            );
        }
    }

    fn collect_fn(
        &self,
        gid: usize,
        file_idx: usize,
        state: &mut TaintState,
        depth: usize,
        stack: &mut Vec<(usize, usize)>,
        merged: &mut BTreeMap<(String, usize, usize, usize, usize), Finding>,
    ) {
        let f = self.fn_fact(gid);
        self.apply_stmts(f, state, &self.summaries);

        // sink 命中：state 为语句处理后的最终状态（存在性近似，Domain §4a.3）。
        for sink in &f.sinks {
            let flows = self.expr_flows(&sink.expr, state);
            if flows.is_empty() {
                continue;
            }
            let file = &self.files[file_idx];
            let key = (
                file.path.clone(),
                sink.span.start.line,
                sink.span.start.column,
                sink.span.end.line,
                sink.span.end.column,
            );
            let mut finding = Finding {
                rule_id: self.rule.spec.id.clone(),
                dimension: self.rule.spec.dimension,
                severity: self.rule.spec.severity,
                path: file.path.clone(),
                span: sink.span,
                message: self.rule.spec.message.clone(),
                match_text: sink.text.clone(),
                captures: Vec::new(),
                degraded: file.degraded,
                flows,
                fingerprint: String::new(),
            };
            finding.flows.sort_by_key(|p| (p.line, p.column));
            finding.flows.dedup();
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

        // 跨函数 sink 展开：本函数内所有调用的实参污点 → 被调函数形参可达 sink。
        if depth >= MAX_CALL_DEPTH {
            return;
        }
        let mut calls: Vec<CallFact> = Vec::new();
        for stmt in &f.stmts {
            gather_stmt_calls(stmt, &mut calls);
        }
        for ret in &f.returns {
            gather_expr_calls(ret, &mut calls);
        }
        for sink in &f.sinks {
            gather_expr_calls(&sink.expr, &mut calls);
        }
        for call in calls {
            if call.sanitized {
                continue;
            }
            let targets = self.resolve(file_idx, &call.name);
            if targets.is_empty() {
                continue;
            }
            for (arg_idx, arg) in call.args.iter().enumerate() {
                let Some(flow) = self.expr_flows(arg, state).first().copied() else {
                    continue;
                };
                for callee in &targets {
                    if !self.summaries[*callee].sink_from_param.contains(&arg_idx) {
                        continue;
                    }
                    if stack.contains(&(*callee, arg_idx)) {
                        continue;
                    }
                    let callee_fact = self.fn_fact(*callee);
                    let Some(name) = callee_fact.params.get(arg_idx) else {
                        continue;
                    };
                    if name.is_empty() {
                        continue;
                    }
                    let mut callee_state = TaintState::default();
                    callee_state.set_whole(name, flow);
                    stack.push((*callee, arg_idx));
                    let callee_file = self.globals[*callee].file_idx;
                    self.collect_fn(
                        *callee,
                        callee_file,
                        &mut callee_state,
                        depth + 1,
                        stack,
                        merged,
                    );
                    stack.pop();
                }
            }
        }
    }

    fn apply_stmts(&self, f: &FnFact, state: &mut TaintState, summaries: &[Summary]) {
        for stmt in &f.stmts {
            match stmt {
                StmtFact::Assign { targets, rhs } => {
                    if rhs.sanitized {
                        for target in targets {
                            self.assign_target(target, None, state);
                        }
                        continue;
                    }
                    if let Some(source_var) = &rhs.alias {
                        for target in targets {
                            state.alias_target(target, source_var);
                        }
                        continue;
                    }
                    let flow = self
                        .expr_flows_with(rhs, state, summaries)
                        .first()
                        .copied();
                    for target in targets {
                        self.assign_target(target, flow, state);
                    }
                }
                StmtFact::ObjectInit { target, fields } => {
                    state.strong_reset(target);
                    for (field, value) in fields {
                        if value.sanitized {
                            continue;
                        }
                        if let Some(source_var) = &value.alias {
                            state.alias_target(&target.field_path(field), source_var);
                            continue;
                        }
                        if let Some(flow) = self
                            .expr_flows_with(value, state, summaries)
                            .first()
                            .copied()
                        {
                            state.set_field(target, std::slice::from_ref(field), Some(flow));
                        }
                    }
                }
                StmtFact::Call(call) => {
                    // 调用值语义由表达式求值处理；此处的独立调用无需改状态。
                    let _ = call;
                }
            }
        }
    }

    fn assign_target(&self, target: &AccessPath, flow: Option<Position>, state: &mut TaintState) {
        if target.fields.is_empty() {
            state.strong_reset(target);
            if let Some(flow) = flow {
                state.set_whole(&target.base, flow);
            }
        } else {
            state.set_field_path(target, flow);
        }
    }

    fn expr_flows(&self, expr: &ExprFact, state: &TaintState) -> Vec<Position> {
        self.expr_flows_with(expr, state, &self.summaries)
    }

    fn expr_flows_with(
        &self,
        expr: &ExprFact,
        state: &TaintState,
        summaries: &[Summary],
    ) -> Vec<Position> {
        let mut flows: Vec<Position> = Vec::new();
        if expr.sanitized {
            return flows;
        }
        flows.extend(expr.sources.iter().copied());
        for path in &expr.paths {
            if let Some(flow) = state.read_flow(path) {
                flows.push(flow);
            }
        }
        for call in &expr.calls {
            // 实参按值求值：调用点上的读取同样构成污点流。
            for arg in &call.args {
                flows.extend(self.expr_flows_with(arg, state, summaries));
            }
            if let Some(flow) = self.call_value_flow(call, state, summaries) {
                flows.push(flow);
            }
        }
        flows.sort_by_key(|p| (p.line, p.column));
        flows.dedup();
        flows
    }

    /// 调用返回值污点：被调内部 source 或「实参污点 → 返回参数依赖」。
    fn call_value_flow(
        &self,
        call: &CallFact,
        state: &TaintState,
        summaries: &[Summary],
    ) -> Option<Position> {
        if call.sanitized {
            return None;
        }
        // 调用名解析需要文件上下文；这里只按「全部可能目标」的并集。
        let mut best: Option<Position> = None;
        for file_idx in 0..self.files.len() {
            let targets = self.resolve(file_idx, &call.name);
            for gid in targets {
                if let Some(pos) = summaries[gid].ret_internal {
                    best = pick_min(best, pos);
                }
                for (idx, arg) in call.args.iter().enumerate() {
                    if !summaries[gid].ret_from_param.contains(&idx) {
                        continue;
                    }
                    if let Some(arg_flow) = self
                        .expr_flows_with(arg, state, summaries)
                        .first()
                        .copied()
                    {
                        best = pick_min(best, arg_flow);
                    }
                }
            }
        }
        best
    }

    fn eval_returns(
        &self,
        f: &FnFact,
        state: &TaintState,
        summaries: &[Summary],
    ) -> Option<Position> {
        let mut best: Option<Position> = None;
        for ret in &f.returns {
            if let Some(flow) = self
                .expr_flows_with(ret, state, summaries)
                .first()
                .copied()
            {
                best = pick_min(best, flow);
            }
        }
        best
    }

    fn any_sink_flow(&self, f: &FnFact, state: &TaintState, summaries: &[Summary]) -> bool {
        f.sinks
            .iter()
            .any(|sink| !self.expr_flows_with(&sink.expr, state, summaries).is_empty())
    }
}

/// 字段敏感 + 对象组别名状态。
#[derive(Default)]
struct TaintState {
    next_group: usize,
    var_group: HashMap<String, usize>,
    /// 组 → 字段路径（点连接）→ 源头位置。
    group_fields: HashMap<usize, HashMap<String, Position>>,
    /// 组整体污点（对象级）。
    group_whole: HashMap<usize, Position>,
}

impl TaintState {
    fn new_group(&mut self) -> usize {
        let g = self.next_group;
        self.next_group += 1;
        g
    }

    fn group_of(&mut self, var: &str) -> usize {
        if let Some(g) = self.var_group.get(var) {
            return *g;
        }
        let g = self.new_group();
        self.var_group.insert(var.to_string(), g);
        g
    }

    fn set_whole(&mut self, var: &str, flow: Position) {
        let g = self.group_of(var);
        self.group_whole.entry(g).or_insert(flow);
    }

    /// 强更新：目标变量进入新组（旧别名保持原组）。
    fn strong_reset(&mut self, target: &AccessPath) {
        if target.fields.is_empty() {
            let g = self.new_group();
            self.var_group.insert(target.base.clone(), g);
        } else {
            self.remove_field(target, true);
        }
    }

    fn alias_target(&mut self, target: &AccessPath, source_var: &str) {
        let source_group = self.group_of(source_var);
        if target.fields.is_empty() {
            self.var_group.insert(target.base.clone(), source_group);
            return;
        }
        // 字段别名：把源组字段复制到目标路径前缀下。
        let copies: Vec<(String, Position)> = self
            .group_fields
            .get(&source_group)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        let whole = self.group_whole.get(&source_group).copied();
        for (field, flow) in copies {
            let mut fields = target.fields.clone();
            fields.extend(field.split('.').map(|s| s.to_string()));
            self.set_field(target, &fields, Some(flow));
        }
        if let Some(flow) = whole {
            self.set_field(target, &target.fields.clone(), Some(flow));
        }
    }

    fn set_field_path(&mut self, target: &AccessPath, flow: Option<Position>) {
        self.set_field(target, &target.fields.clone(), flow);
    }

    fn set_field(&mut self, target: &AccessPath, fields: &[String], flow: Option<Position>) {
        let g = self.group_of(&target.base);
        let full: Vec<String> = target
            .fields
            .iter()
            .cloned()
            .chain(fields.iter().cloned())
            .collect();
        let key = full.join(".");
        match flow {
            Some(flow) => {
                self.group_fields
                    .entry(g)
                    .or_default()
                    .entry(key)
                    .or_insert(flow);
            }
            None => self.remove_field_key(g, &key),
        }
    }

    fn remove_field(&mut self, target: &AccessPath, strong: bool) {
        let Some(&g) = self.var_group.get(&target.base) else {
            return;
        };
        let key = target.fields.join(".");
        if strong {
            self.remove_field_key(g, &key);
        }
    }

    fn remove_field_key(&mut self, group: usize, key: &str) {
        if let Some(map) = self.group_fields.get_mut(&group) {
            map.retain(|k, _| {
                !(k == key || k.starts_with(&format!("{key}.")))
            });
        }
    }

    fn read_flow(&self, path: &AccessPath) -> Option<Position> {
        let &g = self.var_group.get(&path.base)?;
        let key = path.fields.join(".");
        let fields = self.group_fields.get(&g);
        if path.fields.is_empty() {
            // 读取整对象：整体污点或任一字段污点（含通配）。
            let mut best = self.group_whole.get(&g).copied();
            if let Some(map) = fields {
                for pos in map.values() {
                    best = pick_min(best, *pos);
                }
            }
            return best;
        }
        let mut best: Option<Position> = None;
        if let Some(map) = fields {
            for (k, pos) in map {
                let matched = k == "*"
                    || k == &key
                    || k.starts_with(&format!("{key}."))
                    || key.starts_with(&format!("{k}."));
                if matched {
                    best = pick_min(best, *pos);
                }
            }
        }
        best.or_else(|| self.group_whole.get(&g).copied())
    }
}

fn pick_min(current: Option<Position>, candidate: Position) -> Option<Position> {
    match current {
        Some(pos)
            if (pos.line, pos.column) <= (candidate.line, candidate.column) =>
        {
            Some(pos)
        }
        _ => Some(candidate),
    }
}

fn gather_expr_calls(expr: &ExprFact, out: &mut Vec<CallFact>) {
    for call in &expr.calls {
        out.push(call.clone());
        for arg in &call.args {
            gather_expr_calls(arg, out);
        }
    }
}

fn gather_stmt_calls(stmt: &StmtFact, out: &mut Vec<CallFact>) {
    match stmt {
        StmtFact::Assign { rhs, .. } => gather_expr_calls(rhs, out),
        StmtFact::ObjectInit { fields, .. } => {
            for (_, expr) in fields {
                gather_expr_calls(expr, out);
            }
        }
        StmtFact::Call(call) => {
            out.push(call.clone());
            for arg in &call.args {
                gather_expr_calls(arg, out);
            }
        }
    }
}

/// 项目分析入口：规则 + 全部文件事实（按路径排序后传入以保证确定性）。
pub fn analyze_project(rule: &TaintRule, files: &[FileFacts]) -> Vec<Finding> {
    let mut globals: Vec<GlobalFn> = Vec::new();
    let mut module_index: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (file_idx, file) in files.iter().enumerate() {
        for (func_idx, func) in file.functions.iter().enumerate() {
            let gid = globals.len();
            globals.push(GlobalFn { file_idx, func_idx });
            if let Some(name) = &func.name {
                module_index
                    .entry((file.module_id.clone(), name.clone()))
                    .or_default()
                    .push(gid);
            }
        }
    }

    let mut resolution: Vec<HashMap<String, Vec<usize>>> = Vec::with_capacity(files.len());
    for (file_idx, file) in files.iter().enumerate() {
        let mut map: HashMap<String, Vec<usize>> = HashMap::new();
        // 本地函数。
        for (gid, g) in globals.iter().enumerate() {
            if g.file_idx != file_idx {
                continue;
            }
            if let Some(name) = &files[g.file_idx].functions[g.func_idx].name {
                map.entry(name.clone()).or_default().push(gid);
            }
        }
        // Go 同包（目录）跨文件。
        if file.lang == crate::language::Language::Go {
            for ((module, name), gids) in &module_index {
                if module == &file.module_id {
                    map.entry(name.clone()).or_default().extend(gids);
                }
            }
        }
        // 显式导入。
        for import in &file.imports {
            for module in &import.modules {
                if let Some(gids) = module_index.get(&(module.clone(), import.exported.clone())) {
                    map.entry(import.local.clone()).or_default().extend(gids);
                }
            }
        }
        for gids in map.values_mut() {
            gids.sort_unstable();
            gids.dedup();
        }
        resolution.push(map);
    }

    let mut project = Project {
        rule,
        files,
        globals,
        resolution,
        summaries: Vec::new(),
    };
    project.analyze()
}
