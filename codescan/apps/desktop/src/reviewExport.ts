/** LLM 审查建议的展示层工具：筛选与导出（不进入发现/基线/门禁）。 */
import {
  CONFIDENCE_LABEL,
  DIMENSION_LABEL,
  SEVERITY_LABEL,
} from "./labels";
import type { ReviewSuggestion } from "./platform";

export type ReviewFilter = {
  severity: ReviewSuggestion["severity"] | "all";
  category: ReviewSuggestion["category"] | "all";
  confidence: ReviewSuggestion["confidence"] | "all";
  query: string;
};

export const EMPTY_REVIEW_FILTER: ReviewFilter = {
  severity: "all",
  category: "all",
  confidence: "all",
  query: "",
};

export function hasReviewFilter(f: ReviewFilter): boolean {
  return (
    f.severity !== "all" ||
    f.category !== "all" ||
    f.confidence !== "all" ||
    f.query.trim() !== ""
  );
}

/** 按严重级/维度/置信度与路径·标题·说明关键词筛选；大小写不敏感。 */
export function filterSuggestions(
  suggestions: ReviewSuggestion[],
  filter: ReviewFilter,
): ReviewSuggestion[] {
  const query = filter.query.trim().toLowerCase();
  return suggestions.filter(
    (s) =>
      (filter.severity === "all" || s.severity === filter.severity) &&
      (filter.category === "all" || s.category === filter.category) &&
      (filter.confidence === "all" || s.confidence === filter.confidence) &&
      (query === "" ||
        s.path.toLowerCase().includes(query) ||
        s.title.toLowerCase().includes(query) ||
        s.detail.toLowerCase().includes(query)),
  );
}

/** 复核结论标签（未复核时前端不渲染徽标）。 */
export const VERIFICATION_LABEL: Record<
  NonNullable<ReviewSuggestion["verification"]>,
  string
> = {
  confirmed: "已复核",
  needs_validation: "待验证",
};

/** 导出文本头部：声明建议层边界，避免与确定性发现混淆。 */
const EXPORT_HEADER =
  "LLM 源码审查建议（非确定性参考，不参与门禁）\n导出时间不记录；排序：路径 → 行号 → 标题。\n";

/** 导出为 Markdown：与单条「复制」格式一致，另加证据、置信度与复核状态。 */
export function suggestionsToMarkdown(suggestions: ReviewSuggestion[]): string {
  const lines: string[] = [EXPORT_HEADER];
  for (const s of suggestions) {
    lines.push(
      `- ${SEVERITY_LABEL[s.severity]}｜${DIMENSION_LABEL[s.category]}｜${s.path}:${s.line}`,
      `  - ${s.title}`,
      ...(s.detail ? [`  - ${s.detail}`] : []),
      `  - 置信度：${CONFIDENCE_LABEL[s.confidence]}`,
      ...(s.verification
        ? [`  - 复核：${VERIFICATION_LABEL[s.verification]}`]
        : []),
    );
  }
  return lines.join("\n");
}

/** 导出为 JSON 数组（与后端 ReviewSuggestion 字段一致）。 */
export function suggestionsToJson(suggestions: ReviewSuggestion[]): string {
  return JSON.stringify(suggestions, null, 2);
}
