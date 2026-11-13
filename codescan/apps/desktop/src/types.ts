/** 与 Rust 侧 codescan-engine 模型一一对应的类型（Domain Spec §2）。 */

export type Dimension = "security" | "business-logic" | "quality";
export type Severity = "error" | "warning" | "info";

export type Position = { line: number; column: number };
export type Span = { start: Position; end: Position };

export type Finding = {
  rule_id: string;
  dimension: Dimension;
  severity: Severity;
  path: string;
  span: Span;
  message: string;
  match_text: string;
  captures: [string, string][];
  degraded: boolean;
  /** 污点链证据：sink 对应的 source 位置列表（过程内近似）。 */
  flows: Position[];
  fingerprint: string;
};

export type ScanStats = {
  files_total: number;
  files_scanned: number;
  files_ignored: number;
  files_unknown_language: number;
  findings: number;
};
