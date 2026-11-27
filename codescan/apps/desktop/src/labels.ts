/** 维度/严重级/置信度的展示文案（发现页与 LLM 审查共用）。 */
import type { Finding, ScanEvent } from "./types";

export type ReviewSeverity = "error" | "warning" | "info";
export type ReviewCategory = Finding["dimension"];
export type ReviewConfidence = "high" | "medium" | "low";

export const SEVERITY_LABEL: Record<ReviewSeverity, string> = {
  error: "错误",
  warning: "警告",
  info: "提示",
};

export const DIMENSION_LABEL: Record<ReviewCategory, string> = {
  security: "安全",
  "business-logic": "业务逻辑",
  quality: "质量",
};

export const CONFIDENCE_LABEL: Record<ReviewConfidence, string> = {
  high: "高",
  medium: "中",
  low: "低",
};

export function formatScanEvent(event: ScanEvent): string {
  switch (event.kind) {
    case "parse_error":
      return `解析失败：${event.path}（${event.error_nodes} 个错误节点，命中标记 degraded）`;
    case "io_error":
      return `读取失败：${event.path}（${event.message}）`;
    case "binary":
      return `二进制跳过：${event.path}`;
    case "oversize":
      return `超限跳过：${event.path}（${event.size} 字节）`;
    case "ignored_summary":
      return `已忽略 ${event.count} 个文件`;
    case "unknown_language_summary":
      return `未知语言 ${event.count} 个文件（仅执行文本规则）`;
  }
  return "未知扫描事件";
}
