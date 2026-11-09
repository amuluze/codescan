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
    /// (module, name) → 全局函数 id（含 Java class_id 键）。
    module_index: HashMap<(String, String), Vec<usize>>,
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

    /// 调用解析：限定者优先（Java `Class.method` / Rust 模块路径），否则裸名。
    fn resolve_call(&self, file_idx: usize, call: &CallFact) -> Vec<usize> {
        if let Some(qualifier) = &call.qualifier {
            let key = format!("{qualifier}.{}", call.name);
            if let Some(gids) = self.resolution[file_idx].get(&key) {
                return gids.clone();
            }
            let file = &self.files[file_idx];
            if file.lang == crate::language::Language::Rust {
                let mut out = Vec::new();
                for module in super::facts::rust_qualifier_candidates(&file.path, qualifier) {
                    if let Some(gids) =
                        self.module_index.get(&(module, call.name.clone()))
                    {
                        out.extend(gids.iter().copied());
                    }
                }
                if !out.is_empty() {
                    out.sort_unstable();
                    out.dedup();
                    return out;
                }
            }
        }
        self.resolve(file_idx, &call.name)
    }

    fn analyze(&mut self) -> Vec<Finding> {
        self.compute_summaries();
        let mut merged: BTreeMap<(String, usize, usize, usize, usize), Finding> = BTreeMap::new();
        for gid in 0..self.globals.len() {
            let mut state = PointsToState::default();
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
                    let state = PointsToState::default();
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
                    let mut state = PointsToState::default();
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
        state: &mut PointsToState,
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
            let targets = self.resolve_call(file_idx, &call);
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
                    let mut callee_state = PointsToState::default();
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

    fn apply_stmts(&self, f: &FnFact, state: &mut PointsToState, summaries: &[Summary]) {
        for stmt in &f.stmts {
            let StmtFact::Assign { targets, rhs } = stmt else {
                continue;
            };
            for target in targets {
                self.assign(target, rhs, state, summaries);
            }
        }
    }

    fn assign(
        &self,
        target: &AccessPath,
        rhs: &ExprFact,
        state: &mut PointsToState,
        summaries: &[Summary],
    ) {
        if rhs.sanitized {
            state.strong_write(target, None);
            return;
        }
        if let Some(source_var) = &rhs.alias {
            state.alias_assign(target, source_var);
            return;
        }
        if let Some(fields) = &rhs.fields {
            // 形状敏感初始化：目标得到一个新对象，字段递归展开。
            let obj = state.fresh();
            if target.fields.is_empty() {
                state
                    .var_ptrs
                    .insert(target.base.clone(), std::collections::BTreeSet::from([obj]));
            } else {
                let parents = state.objects_for_write_parent(target);
                let field = target.fields.last().cloned().unwrap_or_default();
                for parent in parents {
                    state.write_field_ptr(parent, &field, &std::collections::BTreeSet::from([obj]));
                }
            }
            self.init_shape(obj, fields, state, summaries);
            return;
        }
        let flow = self
            .expr_flows_with(rhs, state, summaries)
            .first()
            .copied();
        state.strong_write(target, flow);
    }

    fn init_shape(
        &self,
        obj: usize,
        fields: &[(String, ExprFact)],
        state: &mut PointsToState,
        summaries: &[Summary],
    ) {
        for (key, value) in fields {
            if value.sanitized {
                continue;
            }
            if let Some(nested) = &value.fields {
                let child = state.fresh();
                state.write_field_ptr(obj, key, &std::collections::BTreeSet::from([child]));
                self.init_shape(child, nested, state, summaries);
                continue;
            }
            if let Some(var) = &value.alias {
                let set = state.objects_for_alias(var);
                state.write_field_ptr(obj, key, &set);
                continue;
            }
            if let Some(flow) = self
                .expr_flows_with(value, state, summaries)
                .first()
                .copied()
            {
                state.write_field_flow(obj, key, Some(flow));
            }
        }
    }

    fn expr_flows(&self, expr: &ExprFact, state: &PointsToState) -> Vec<Position> {
        self.expr_flows_with(expr, state, &self.summaries)
    }

    fn expr_flows_with(
        &self,
        expr: &ExprFact,
        state: &PointsToState,
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
        state: &PointsToState,
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
        state: &PointsToState,
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

    fn any_sink_flow(&self, f: &FnFact, state: &PointsToState, summaries: &[Summary]) -> bool {
        f.sinks
            .iter()
            .any(|sink| !self.expr_flows_with(&sink.expr, state, summaries).is_empty())
    }
}

/// Points-to 对象集合 + 形状敏感字段图。
#[derive(Default)]
struct PointsToState {
    next_obj: usize,
    /// 变量 → 抽象对象集合。
    var_ptrs: HashMap<String, std::collections::BTreeSet<usize>>,
    /// 对象 → 字段 → (指向对象集合, 字段污点)。
    obj_fields: HashMap<usize, HashMap<String, FieldValue>>,
    /// 对象整体污点（来自标量赋值/参数源）。
    obj_whole: HashMap<usize, Position>,
}

#[derive(Default, Clone)]
struct FieldValue {
    ptrs: std::collections::BTreeSet<usize>,
    flow: Option<Position>,
}

impl PointsToState {
    fn fresh(&mut self) -> usize {
        let id = self.next_obj;
        self.next_obj += 1;
        id
    }

    /// 形参/新对象种子：新对象并记录整体污点。
    fn set_whole(&mut self, var: &str, flow: Position) {
        let obj = self.fresh();
        self.var_ptrs
            .insert(var.to_string(), std::collections::BTreeSet::from([obj]));
        self.obj_whole.insert(obj, flow);
    }

    fn objects_for_alias(&mut self, var: &str) -> std::collections::BTreeSet<usize> {
        if let Some(set) = self.var_ptrs.get(var) {
            if !set.is_empty() {
                return set.clone();
            }
        }
        let obj = self.fresh();
        self.var_ptrs
            .entry(var.to_string())
            .or_default()
            .insert(obj);
        std::collections::BTreeSet::from([obj])
    }

    /// 强更新：裸目标 → 新对象（可带整体污点）；字段目标 → 字段强更新。
    fn strong_write(&mut self, target: &AccessPath, flow: Option<Position>) {
        if target.fields.is_empty() {
            let obj = self.fresh();
            self.var_ptrs
                .insert(target.base.clone(), std::collections::BTreeSet::from([obj]));
            if let Some(flow) = flow {
                self.obj_whole.insert(obj, flow);
            }
            return;
        }
        let parents = self.objects_for_write_parent(target);
        let field = target.fields.last().cloned().unwrap_or_default();
        for parent in parents {
            self.write_field_flow(parent, &field, flow);
        }
    }

    /// 别名赋值：`a = b` 合并对象集合；字段目标共享字段对象。
    fn alias_assign(&mut self, target: &AccessPath, source_var: &str) {
        let set = self.objects_for_alias(source_var);
        if target.fields.is_empty() {
            self.var_ptrs.insert(target.base.clone(), set);
            return;
        }
        let parents = self.objects_for_write_parent(target);
        let field = target.fields.last().cloned().unwrap_or_default();
        for parent in parents {
            self.write_field_ptr(parent, &field, &set);
        }
    }

    /// 写目标父路径 → 对象集合（缺失的中间字段创建新对象并挂接）。
    fn objects_for_write_parent(&mut self, target: &AccessPath) -> std::collections::BTreeSet<usize> {
        let mut current = self.objects_for_alias(&target.base);
        for seg in &target.fields[..target.fields.len().saturating_sub(1)] {
            let mut next = std::collections::BTreeSet::new();
            for obj in current {
                let existing: Vec<usize> = self
                    .obj_fields
                    .get(&obj)
                    .and_then(|f| f.get(seg))
                    .map(|e| e.ptrs.iter().copied().collect())
                    .unwrap_or_default();
                if !existing.is_empty() {
                    next.extend(existing);
                    continue;
                }
                let child = self.fresh();
                let entry = self
                    .obj_fields
                    .entry(obj)
                    .or_default()
                    .entry(seg.clone())
                    .or_default();
                entry.ptrs.insert(child);
                next.insert(child);
            }
            current = next;
        }
        current
    }

    fn write_field_flow(&mut self, obj: usize, field: &str, flow: Option<Position>) {
        let fields = self.obj_fields.entry(obj).or_default();
        // 强更新：移除该字段及其子路径。
        fields.retain(|k, _| !(k == field || k.starts_with(&format!("{field}."))));
        if let Some(flow) = flow {
            fields.insert(
                field.to_string(),
                FieldValue {
                    ptrs: Default::default(),
                    flow: Some(flow),
                },
            );
        }
    }

    fn write_field_ptr(
        &mut self,
        obj: usize,
        field: &str,
        ptrs: &std::collections::BTreeSet<usize>,
    ) {
        let fields = self.obj_fields.entry(obj).or_default();
        let entry = fields.entry(field.to_string()).or_default();
        entry.ptrs.extend(ptrs.iter().copied());
    }

    fn read_flow(&self, path: &AccessPath) -> Option<Position> {
        let set = self.var_ptrs.get(&path.base)?;
        if path.fields.is_empty() {
            let mut best: Option<Position> = None;
            let mut visited = std::collections::BTreeSet::new();
            for obj in set {
                best = pick_min(best, self.object_taint(*obj, &mut visited).unwrap_or(Position { line: usize::MAX, column: usize::MAX }))
                    .filter(|p| p.line != usize::MAX);
            }
            return best;
        }
        let mut current: std::collections::BTreeSet<usize> = set.clone();
        let mut best: Option<Position> = None;
        for (idx, seg) in path.fields.iter().enumerate() {
            let mut next = std::collections::BTreeSet::new();
            for obj in &current {
                if let Some(flow) = self.obj_whole.get(obj) {
                    best = pick_min(best, *flow);
                }
                if let Some(fields) = self.obj_fields.get(obj) {
                    for (k, value) in fields {
                        // 通配读（`obj[*]`）匹配任意字段；通配写字段匹配任意读。
                        let matched = seg == "*"
                            || k == "*"
                            || k == seg
                            || k.starts_with(&format!("{seg}."))
                            || seg.starts_with(&format!("{k}."));
                        if !matched {
                            continue;
                        }
                        if let Some(flow) = value.flow {
                            best = pick_min(best, flow);
                        }
                        next.extend(value.ptrs.iter().copied());
                    }
                }
            }
            current = next;
            if current.is_empty() {
                break;
            }
            if idx + 1 == path.fields.len() {
                // 末端字段指向的对象若整体有污点，读取该字段值同样命中。
                for obj in &current {
                    if let Some(flow) = self.obj_whole.get(obj) {
                        best = pick_min(best, *flow);
                    }
                }
            }
        }
        best
    }

    /// 对象可达范围内的任一污点（整体/字段/子对象），用于整对象读取。
    fn object_taint(
        &self,
        obj: usize,
        visited: &mut std::collections::BTreeSet<usize>,
    ) -> Option<Position> {
        if !visited.insert(obj) {
            return None;
        }
        let mut best = self.obj_whole.get(&obj).copied();
        if let Some(fields) = self.obj_fields.get(&obj) {
            for value in fields.values() {
                if let Some(flow) = value.flow {
                    best = pick_min(best, flow);
                }
                for child in &value.ptrs {
                    if let Some(flow) = self.object_taint(*child, visited) {
                        best = pick_min(best, flow);
                    }
                }
            }
        }
        best
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
                if let Some(class_id) = &file.class_id {
                    module_index
                        .entry((class_id.clone(), name.clone()))
                        .or_default()
                        .push(gid);
                }
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
            let mut plain_added = false;
            for module in &import.modules {
                if let Some(gids) = module_index.get(&(module.clone(), import.exported.clone())) {
                    map.entry(import.local.clone()).or_default().extend(gids);
                    plain_added = true;
                }
            }
            // Java 类导入：为类内每个方法登记限定键 "Class.method"。
            if file.lang == crate::language::Language::Java && !plain_added {
                for module in &import.modules {
                    let methods: Vec<(String, Vec<usize>)> = module_index
                        .iter()
                        .filter(|((m, _), _)| m == module)
                        .map(|((_, name), gids)| (name.clone(), gids.clone()))
                        .collect();
                    for (name, gids) in methods {
                        map.entry(format!("{}.{}", import.local, name))
                            .or_default()
                            .extend(gids);
                    }
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
        module_index,
        resolution,
        summaries: Vec::new(),
    };
    project.analyze()
}
