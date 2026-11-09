//! 指纹赋值（Domain §2.3）：`规则id ‖ 相对路径 ‖ 内容规范指纹 ‖ 出现序号`。
//! 出现序号按组内命中位置排序赋值，保证删除出现得前缀、新增恰一条。

use crate::model::{Finding, Span};
use sha2::{Digest, Sha256};

/// 内容规范化：CRLF 归一、行尾空白剔除。内部空白保留。
pub fn normalize(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn content_hash(normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn span_sort_key(span: &Span) -> (usize, usize, usize, usize) {
    (
        span.start.line,
        span.start.column,
        span.end.line,
        span.end.column,
    )
}

/// 排序 + 指纹赋值。输入为未排序的原始发现，输出满足：
/// 1. 按（路径, span, 规则 id）全序；
/// 2. 每条发现的指纹 = `rule\u{1}path\u{1}sha256(norm(match_text))\u{1}idx`；
///    idx 为同 (rule, path, content) 组内按 span 排序的序号。
pub fn finalize(raw: Vec<Finding>) -> Vec<Finding> {
    let mut findings = raw;

    // 第一遍：按分组键排序并赋出现序号。
    findings.sort_by(|a, b| {
        let content_a = content_hash(&normalize(&a.match_text));
        let content_b = content_hash(&normalize(&b.match_text));
        (
            a.rule_id.clone(),
            a.path.clone(),
            content_a,
            span_sort_key(&a.span),
        )
            .cmp(&(
                b.rule_id.clone(),
                b.path.clone(),
                content_b,
                span_sort_key(&b.span),
            ))
    });

    let mut prev_group: Option<(String, String, String)> = None;
    let mut index: usize = 0;
    for finding in &mut findings {
        let content = content_hash(&normalize(&finding.match_text));
        let group = (finding.rule_id.clone(), finding.path.clone(), content.clone());
        if prev_group.as_ref() == Some(&group) {
            index += 1;
        } else {
            index = 0;
            prev_group = Some(group);
        }
        let fingerprint_input = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}",
            finding.rule_id, finding.path, content, index
        );
        let mut hasher = Sha256::new();
        hasher.update(fingerprint_input.as_bytes());
        finding.fingerprint = format!("{:x}", hasher.finalize());
    }

    // 第二遍：报告全序。
    findings.sort_by(|a, b| {
        (
            a.path.clone(),
            span_sort_key(&a.span),
            a.rule_id.clone(),
        )
            .cmp(&(b.path.clone(), span_sort_key(&b.span), b.rule_id.clone()))
    });
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dimension, Position, Severity};

    fn raw_finding(rule: &str, path: &str, line: usize, text: &str) -> Finding {
        Finding {
            rule_id: rule.into(),
            dimension: Dimension::Quality,
            severity: Severity::Warning,
            path: path.into(),
            span: Span {
                start: Position { line, column: 1 },
                end: Position { line, column: 10 },
            },
            message: "m".into(),
            match_text: text.into(),
            captures: Vec::new(),
            degraded: false,
            fingerprint: String::new(),
        }
    }

    #[test]
    fn occurrence_index_prefix_stable_on_delete() {
        let base = finalize(vec![
            raw_finding("r", "a.ts", 1, "eval(x)"),
            raw_finding("r", "a.ts", 5, "eval(x)"),
            raw_finding("r", "a.ts", 9, "eval(x)"),
        ]);
        let fps_full: Vec<&str> = base.iter().map(|f| f.fingerprint.as_str()).collect();

        // 删除中间一次出现（行 5）。
        let reduced = finalize(vec![
            raw_finding("r", "a.ts", 1, "eval(x)"),
            raw_finding("r", "a.ts", 9, "eval(x)"),
        ]);
        let fps_reduced: Vec<&str> = reduced.iter().map(|f| f.fingerprint.as_str()).collect();

        // 删除后剩余发现的指纹集合是原集合的子集（不产生新增）。
        for fp in &fps_reduced {
            assert!(fps_full.contains(fp), "删除出现后产生了新指纹 {fp}");
        }
        assert_eq!(fps_reduced.len(), 2);
    }

    #[test]
    fn adding_occurrence_adds_at_most_one() {
        let before = finalize(vec![raw_finding("r", "a.ts", 1, "eval(x)")]);
        let after = finalize(vec![
            raw_finding("r", "a.ts", 1, "eval(x)"),
            raw_finding("r", "a.ts", 3, "eval(x)"),
        ]);
        let new_count = after
            .iter()
            .filter(|f| !before.iter().any(|b| b.fingerprint == f.fingerprint))
            .count();
        assert_eq!(new_count, 1);
    }

    #[test]
    fn line_drift_keeps_fingerprint() {
        let a = finalize(vec![raw_finding("r", "a.ts", 1, "eval(x)")]);
        let b = finalize(vec![raw_finding("r", "a.ts", 42, "eval(x)")]);
        assert_eq!(a[0].fingerprint, b[0].fingerprint);
    }

    #[test]
    fn whitespace_only_change_keeps_fingerprint() {
        let a = finalize(vec![raw_finding("r", "a.ts", 1, "eval(x)")]);
        let b = finalize(vec![raw_finding("r", "a.ts", 1, "eval(x)   ")]);
        assert_eq!(a[0].fingerprint, b[0].fingerprint);
    }

    #[test]
    fn report_order_is_path_span_rule() {
        let out = finalize(vec![
            raw_finding("b-rule", "z.ts", 1, "x"),
            raw_finding("a-rule", "z.ts", 9, "x"),
            raw_finding("z-rule", "a.ts", 100, "x"),
        ]);
        let ids: Vec<&str> = out.iter().map(|f| f.rule_id.as_str()).collect();
        assert_eq!(ids, ["z-rule", "b-rule", "a-rule"]);
    }
}
