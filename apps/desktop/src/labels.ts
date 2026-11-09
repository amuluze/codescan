/** 维度/严重级/置信度的展示文案（发现页与 LLM 审查共用）。 */
import type { Finding } from "./types";

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
