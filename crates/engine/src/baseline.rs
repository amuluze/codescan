//! 基线（Domain §5）：指纹集合的冻结快照；门禁差异 = 新扫描 \ 基线，
//! 仅依赖指纹相等性，degraded 与正常发现同语义参与。

use crate::model::{Finding, ScanReport};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub engine_version: String,
    /// 有序指纹集合（创建时排序冻结）。
    pub entries: Vec<String>,
}

impl Baseline {
    pub fn snapshot(report: &ScanReport) -> Baseline {
        let mut entries: Vec<String> = report
            .findings
            .iter()
            .map(|f| f.fingerprint.clone())
            .collect();
        entries.sort();
        entries.dedup();
        Baseline {
            engine_version: report.engine_version.clone(),
            entries,
        }
    }

    pub fn contains(&self, finding: &Finding) -> bool {
        self.entries.binary_search(&finding.fingerprint).is_ok()
    }

    /// 门禁差异：不在基线中的发现，保持报告全序。
    pub fn gate_new<'a>(&self, report: &'a ScanReport) -> Vec<&'a Finding> {
        report
            .findings
            .iter()
            .filter(|f| !self.contains(f))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::finalize;
    use crate::model::{Dimension, Position, Severity};

    fn report_with_occurrences(occurrences: &[(usize, &str)]) -> ScanReport {
        let findings = finalize(
            occurrences
                .iter()
                .map(|&(line, text)| Finding {
                    rule_id: "r".into(),
                    dimension: Dimension::Quality,
                    severity: Severity::Warning,
                    path: "a.ts".into(),
                    span: crate::model::Span {
                        start: Position { line, column: 1 },
                        end: Position { line, column: 9 },
                    },
                    message: "m".into(),
                    match_text: text.into(),
                    captures: Vec::new(),
                    degraded: false,
                    fingerprint: String::new(),
                })
                .collect(),
        );
        ScanReport {
            engine_version: "test".into(),
            findings,
            events: Vec::new(),
            stats: crate::model::ScanStats::default(),
        }
    }

    fn report_with(lines: &[usize]) -> ScanReport {
        report_with_occurrences(&lines.iter().map(|&l| (l, "eval(x)")).collect::<Vec<_>>())
    }

    #[test]
    fn gate_diff_is_set_difference() {
        let base_report = report_with(&[1, 5, 9]);
        let baseline = Baseline::snapshot(&base_report);

        // 新扫描：行 5 修复，行 12 新增。
        let new_report = report_with_occurrences(&[(1, "eval(x)"), (9, "eval(x)"), (12, "eval(y)")]);
        let gate = baseline.gate_new(&new_report);
        assert_eq!(gate.len(), 1);
        assert_eq!(gate[0].span.start.line, 12);
    }

    #[test]
    fn removing_baseline_entry_restores_gate() {
        let base_report = report_with(&[1, 5]);
        let mut baseline = Baseline::snapshot(&base_report);
        let fp_line5 = base_report.findings[1].fingerprint.clone();
        baseline.entries.retain(|fp| fp != &fp_line5);

        let new_report = report_with(&[1, 5]);
        let gate = baseline.gate_new(&new_report);
        assert_eq!(gate.len(), 1);
        assert_eq!(gate[0].span.start.line, 5);
    }
}
