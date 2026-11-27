//! v2 类型层：Java 类型声明与继承闭包（Domain §4c）。
//! 仅解析类声明、extends/implements 与类内方法；不做泛型/内部类语义证明。

use crate::compile::InheritanceRule;
use crate::language::Language;
use crate::metrics::{named_children, span_of};
use crate::model::{Finding, Span};
use std::collections::{BTreeMap, HashMap, HashSet};
use tree_sitter::Node;

#[derive(Debug, Clone)]
pub struct TypeFacts {
    pub path: String,
    pub package: String,
    /// 简单类名 → 全限定名（`import a.b.C`）。
    pub imports: HashMap<String, String>,
    /// 通配包导入（`import a.b.*`）。
    pub wildcard_packages: Vec<String>,
    pub classes: Vec<ClassFact>,
    pub degraded: bool,
}

#[derive(Debug, Clone)]
pub struct ClassFact {
    pub id: String,
    pub name: String,
    pub extends: Vec<String>,
    pub implements: Vec<String>,
    pub methods: Vec<MethodFact>,
}

#[derive(Debug, Clone)]
pub struct MethodFact {
    pub name: String,
    pub span: Span,
    pub text: String,
    pub empty: bool,
}

pub fn extract_file(
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> Option<TypeFacts> {
    if lang != Language::Java {
        return None;
    }
    let root = tree.root_node();
    let mut package = String::new();
    let mut imports = HashMap::new();
    let mut wildcard_packages = Vec::new();
    for child in named_children(root) {
        match child.kind() {
            "package_declaration" => {
                package = source
                    .get(child.start_byte()..child.end_byte())
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("package")
                    .trim()
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
            }
            "import_declaration" => {
                let raw = source
                    .get(child.start_byte()..child.end_byte())
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("import")
                    .trim()
                    .trim_end_matches(';')
                    .trim();
                if raw.starts_with("static ") {
                    continue;
                }
                if let Some(package) = raw.strip_suffix(".*") {
                    wildcard_packages.push(package.to_string());
                } else if let Some((_, local)) = raw.rsplit_once('.') {
                    imports.insert(local.to_string(), raw.to_string());
                }
            }
            _ => {}
        }
    }

    let mut classes = Vec::new();
    collect_classes(root, &package, source, path, &mut classes);
    Some(TypeFacts {
        path: path.to_string(),
        package,
        imports,
        wildcard_packages,
        classes,
        degraded,
    })
}

fn collect_classes(
    root: Node<'_>,
    package: &str,
    source: &str,
    path: &str,
    out: &mut Vec<ClassFact>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "class_declaration" => {
                if let Some(class) = class_fact(node, package, source) {
                    out.push(class);
                }
            }
            "interface_declaration" => {
                if let Some(interface) = interface_fact(node, package, source) {
                    out.push(interface);
                }
            }
            "object_creation_expression" => {
                if let Some(anonymous) = anonymous_fact(node, package, source, path) {
                    out.push(anonymous);
                }
            }
            _ => {}
        }
        let mut children = named_children(node);
        children.reverse();
        stack.extend(children);
    }
}

fn class_fact(node: Node<'_>, package: &str, source: &str) -> Option<ClassFact> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| source.get(name.start_byte()..name.end_byte()))
        .map(|name| name.to_string())?;
    let id = type_id(package, &name);
    let extends = node
        .child_by_field_name("superclass")
        .map(|node| type_refs(node, source))
        .unwrap_or_default();
    let implements = node
        .child_by_field_name("interfaces")
        .map(|node| type_refs(node, source))
        .unwrap_or_default();
    let methods = node
        .child_by_field_name("body")
        .map(|body| methods_of(body, source))
        .unwrap_or_default();
    Some(ClassFact {
        id,
        name,
        extends,
        implements,
        methods,
    })
}

fn interface_fact(node: Node<'_>, package: &str, source: &str) -> Option<ClassFact> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| source.get(name.start_byte()..name.end_byte()))
        .map(|name| name.to_string())?;
    let id = type_id(package, &name);
    let extends = named_children(node)
        .into_iter()
        .find(|child| child.kind() == "extends_interfaces")
        .map(|node| type_refs(node, source))
        .unwrap_or_default();
    Some(ClassFact {
        id,
        name,
        extends,
        implements: Vec::new(),
        methods: Vec::new(),
    })
}

fn anonymous_fact(
    node: Node<'_>,
    package: &str,
    source: &str,
    path: &str,
) -> Option<ClassFact> {
    let body = named_children(node)
        .into_iter()
        .find(|child| child.kind() == "class_body")?;
    let created = node.child_by_field_name("type")?;
    let start = node.start_position();
    let name = format!("<anonymous@{}:{}>", start.row + 1, start.column + 1);
    let id = format!("{path}#{name}");
    Some(ClassFact {
        id: type_id(package, &id),
        name,
        extends: type_refs(created, source),
        implements: Vec::new(),
        methods: methods_of(body, source),
    })
}

fn methods_of(body: Node<'_>, source: &str) -> Vec<MethodFact> {
    let mut methods = Vec::new();
    for child in named_children(body) {
        if child.kind() != "method_declaration" {
            continue;
        }
        let Some(method_name) = child
            .child_by_field_name("name")
            .and_then(|name| source.get(name.start_byte()..name.end_byte()))
        else {
            continue;
        };
        let empty = child
            .child_by_field_name("body")
            .map(is_empty_block)
            .unwrap_or(true);
        methods.push(MethodFact {
            name: method_name.to_string(),
            span: span_of(child),
            text: source
                .get(child.start_byte()..child.end_byte())
                .unwrap_or("")
                .to_string(),
            empty,
        });
    }
    methods
}

fn type_id(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    }
}

fn is_empty_block(block: Node<'_>) -> bool {
    named_children(block).iter().all(|child| {
        matches!(
            child.kind(),
            "comment" | "line_comment" | "block_comment"
        )
    })
}

fn type_refs(node: Node<'_>, source: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "type_identifier" | "scoped_type_identifier" => {
                if let Some(text) = source.get(node.start_byte()..node.end_byte()) {
                    refs.push(text.to_string());
                }
            }
            // 泛型实参不是父类型；只沿基础类型解析。
            "generic_type" => {
                if let Some(base) = node.named_child(0) {
                    stack.push(base);
                }
            }
            _ => stack.extend(named_children(node)),
        }
    }
    refs.sort();
    refs.dedup();
    refs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolved {
    Local,
    Target,
    Unknown,
}

pub fn analyze(rule: &InheritanceRule, files: &[TypeFacts]) -> Vec<Finding> {
    let mut classes: BTreeMap<String, (&TypeFacts, &ClassFact)> = BTreeMap::new();
    let mut simple: HashMap<String, Vec<String>> = HashMap::new();
    for facts in files {
        for class in &facts.classes {
            classes.entry(class.id.clone()).or_insert((facts, class));
            simple
                .entry(class.name.clone())
                .or_default()
                .push(class.id.clone());
        }
    }
    for ids in simple.values_mut() {
        ids.sort();
        ids.dedup();
    }

    let mut direct_target = HashSet::new();
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    for (id, (facts, class)) in &classes {
        let mut local_parents = Vec::new();
        for reference in class.implements.iter().chain(class.extends.iter()) {
            match resolve(
                reference,
                &facts.package,
                &facts.imports,
                &facts.wildcard_packages,
                &classes,
                &simple,
                &rule.target,
            ) {
                (Resolved::Target, _) => {
                    direct_target.insert(id.clone());
                }
                (Resolved::Local, Some(parent)) => local_parents.push(parent),
                _ => {}
            }
        }
        local_parents.sort();
        local_parents.dedup();
        parents.insert(id.clone(), local_parents);
    }

    let mut memo: HashMap<String, bool> = HashMap::new();
    let mut visiting = HashSet::new();
    let mut findings = Vec::new();
    for (id, (facts, class)) in &classes {
        let reaches = reaches_target(
            id,
            &parents,
            &direct_target,
            &mut memo,
            &mut visiting,
        );
        if !reaches {
            continue;
        }
        if rule.indirect_only && direct_target.contains(id) {
            continue;
        }
        for method in &class.methods {
            if !rule.methods.iter().any(|name| name == &method.name) {
                continue;
            }
            if rule.empty_only && !method.empty {
                continue;
            }
            findings.push(Finding {
                rule_id: rule.spec.id.clone(),
                dimension: rule.spec.dimension,
                severity: rule.spec.severity,
                path: facts.path.clone(),
                span: method.span,
                message: rule.spec.message.clone(),
                match_text: method.text.clone(),
                captures: Vec::new(),
                degraded: facts.degraded,
                flows: Vec::new(),
                fingerprint: String::new(),
            });
        }
    }
    findings
}

fn reaches_target(
    id: &str,
    parents: &HashMap<String, Vec<String>>,
    direct: &HashSet<String>,
    memo: &mut HashMap<String, bool>,
    visiting: &mut HashSet<String>,
) -> bool {
    if let Some(value) = memo.get(id) {
        return *value;
    }
    if direct.contains(id) {
        memo.insert(id.to_string(), true);
        return true;
    }
    if !visiting.insert(id.to_string()) {
        return false;
    }
    let reached = parents
        .get(id)
        .map(|children| {
            children
                .iter()
                .any(|parent| reaches_target(parent, parents, direct, memo, visiting))
        })
        .unwrap_or(false);
    visiting.remove(id);
    memo.insert(id.to_string(), reached);
    reached
}

fn resolve(
    reference: &str,
    package: &str,
    imports: &HashMap<String, String>,
    wildcard_packages: &[String],
    classes: &BTreeMap<String, (&TypeFacts, &ClassFact)>,
    simple: &HashMap<String, Vec<String>>,
    target: &str,
) -> (Resolved, Option<String>) {
    let reference = reference.split('<').next().unwrap_or(reference).trim();
    if reference.is_empty() {
        return (Resolved::Unknown, None);
    }
    if is_target(reference, target) && reference.contains('.') {
        if classes.contains_key(reference) {
            return (Resolved::Local, Some(reference.to_string()));
        }
        return (Resolved::Target, None);
    }
    if reference.contains('.') {
        if classes.contains_key(reference) {
            return (Resolved::Local, Some(reference.to_string()));
        }
        return (Resolved::Unknown, None);
    }
    let same_package = if package.is_empty() {
        reference.to_string()
    } else {
        format!("{package}.{reference}")
    };
    if classes.contains_key(&same_package) {
        return (Resolved::Local, Some(same_package));
    }
    if let Some(imported) = imports.get(reference) {
        if classes.contains_key(imported) {
            return (Resolved::Local, Some(imported.clone()));
        }
        if is_target(imported, target) {
            return (Resolved::Target, None);
        }
    }
    for wildcard in wildcard_packages {
        let candidate = format!("{wildcard}.{reference}");
        if classes.contains_key(&candidate) {
            return (Resolved::Local, Some(candidate));
        }
        if is_target(&candidate, target) {
            return (Resolved::Target, None);
        }
    }
    if let Some(ids) = simple.get(reference) {
        if ids.len() == 1 {
            return (Resolved::Local, Some(ids[0].clone()));
        }
    }
    if is_target(reference, target) {
        return (Resolved::Target, None);
    }
    (Resolved::Unknown, None)
}

fn is_target(reference: &str, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() {
        return false;
    }
    reference == target
        || reference
            .rsplit_once('.')
            .map(|(_, name)| name == target)
            .unwrap_or(false)
}
