//! SARIF 2.1.0 导出（Domain §6.2）：与内部发现一一对应，
//! level 映射 error→error / warning→warning / info→note，degraded 走 properties。

use crate::model::{Finding, ScanReport};
use serde_json::{json, Value};

pub fn export(report: &ScanReport) -> Value {
    let mut rule_ids: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.rule_id.as_str())
        .collect();
    rule_ids.sort_unstable();
    rule_ids.dedup();

    let rules: Vec<Value> = rule_ids
        .iter()
        .map(|id| {
            let message = report
                .findings
                .iter()
                .find(|f| f.rule_id == *id)
                .map(|f| f.message.clone())
                .unwrap_or_default();
            json!({
                "id": id,
                "shortDescription": { "text": message },
            })
        })
        .collect();

    let results: Vec<Value> = report
        .findings
        .iter()
        .map(finding_to_result)
        .collect();

    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "Codescan",
                    "version": report.engine_version,
                    "rules": rules,
                }
            },
            "results": results,
        }],
    })
}

fn finding_to_result(f: &Finding) -> Value {
    json!({
        "ruleId": f.rule_id,
        "level": f.severity.to_sarif_level(),
        "message": { "text": f.message },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": { "uri": f.path },
                "region": {
                    "startLine": f.span.start.line,
                    "startColumn": f.span.start.column,
                    "endLine": f.span.end.line,
                    "endColumn": f.span.end.column,
                },
            },
        }],
        "properties": {
            "degraded": f.degraded,
            "fingerprint": f.fingerprint,
            "dimension": f.dimension,
            "flows": f.flows.iter()
                .map(|p| json!({ "startLine": p.line, "startColumn": p.column }))
                .collect::<Vec<_>>(),
            "captures": f.captures.iter()
                .map(|(k, v)| json!({ "name": k, "text": v }))
                .collect::<Vec<_>>(),
        },
    })
}
