//! 语言注册表：扩展名识别 + tree-sitter 语法接入。
//! 某语言缺席不影响其他语言（Domain §4.2）。

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Python,
    Java,
    Go,
    Rust,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext {
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "jsx" => Some(Language::Jsx),
            "py" => Some(Language::Python),
            "java" => Some(Language::Java),
            "go" => Some(Language::Go),
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    pub fn detect(path: &Path) -> Option<Language> {
        let ext = path.extension()?.to_str()?;
        Self::from_extension(&ext.to_ascii_lowercase())
    }

    /// 规则 YAML 的 language 枚举值（Task Spec 冻结）。
    pub fn from_rule_value(value: &str) -> Option<Language> {
        match value {
            "typescript" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "javascript" => Some(Language::JavaScript),
            "jsx" => Some(Language::Jsx),
            "python" => Some(Language::Python),
            "java" => Some(Language::Java),
            "go" => Some(Language::Go),
            "rust" => Some(Language::Rust),
            _ => None,
        }
    }

    pub fn rule_value(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::JavaScript => "javascript",
            Language::Jsx => "jsx",
            Language::Python => "python",
            Language::Java => "java",
            Language::Go => "go",
            Language::Rust => "rust",
        }
    }

    pub fn tree_sitter_language(self) -> tree_sitter::Language {
        match self {
            Language::TypeScript => {
                tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
            }
            Language::Tsx => tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TSX),
            Language::JavaScript | Language::Jsx => {
                tree_sitter::Language::from(tree_sitter_javascript::LANGUAGE)
            }
            Language::Python => tree_sitter::Language::from(tree_sitter_python::LANGUAGE),
            Language::Java => tree_sitter::Language::from(tree_sitter_java::LANGUAGE),
            Language::Go => tree_sitter::Language::from(tree_sitter_go::LANGUAGE),
            Language::Rust => tree_sitter::Language::from(tree_sitter_rust::LANGUAGE),
        }
    }
}

/// 单遍扫描的 per-file 产物：树 + 源文本 + 降级标记。
pub struct ParsedFile {
    pub tree: tree_sitter::Tree,
    pub source: String,
    pub error_nodes: usize,
}

pub fn parse(lang: Language, bytes: &[u8]) -> ParsedFile {
    let mut parser = tree_sitter::Parser::new();
    // 语法注册失败属引擎构建错误，panic 即 bug（fail-fast 于开发期）。
    parser
        .set_language(&lang.tree_sitter_language())
        .expect("tree-sitter grammar registration failed");
    let tree = parser.parse(bytes, None).expect("parser returned None");
    let error_nodes = count_error_nodes(tree.root_node());
    ParsedFile {
        tree,
        source: String::from_utf8_lossy(bytes).into_owned(),
        error_nodes,
    }
}

/// 统计 ERROR/缺失节点数（用于 ParseError 事件与 degraded 标记）。
fn count_error_nodes(root: tree_sitter::Node<'_>) -> usize {
    let mut count = 0usize;
    let mut cursor = root.walk();
    let mut reached_root = false;
    while !reached_root {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            count += 1;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                reached_root = true;
                break;
            }
        }
    }
    count
}
