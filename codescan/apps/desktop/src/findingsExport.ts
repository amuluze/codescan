/** 发现页导出：仅消费当前视图（筛选后）的确定性发现；不写库、不改变门禁。 */
import { DIMENSION_LABEL, SEVERITY_LABEL } from "./labels";
import type { Finding } from "./types";

/** 导出头部：声明来源与排序，不记录导出时间（Domain §2 确定性）。 */
const EXPORT_HEADER =
  "确定性扫描发现（规则引擎结果）\n导出内容与发现页当前筛选/排序一致；不包含导出时间。\n";

/** Markdown：每条发现含严重级/维度/位置/规则/说明/命中文本/污点链/指纹。 */
export function findingsToMarkdown(findings: Finding[]): string {
  const lines: string[] = [EXPORT_HEADER];
  for (const f of findings) {
    const position = `${f.path}:${f.span.start.line}:${f.span.start.column}`;
    lines.push(
      `- ${SEVERITY_LABEL[f.severity]}｜${DIMENSION_LABEL[f.dimension]}｜${position}`,
      `  - ${f.rule_id}：${f.message}`,
      ...(f.match_text ? [`  - 命中：${f.match_text}`] : []),
      ...(f.flows.length > 0
        ? [`  - 污点链：${f.flows.map((p) => `${p.line}:${p.column}`).join(" → ")}`]
        : []),
      `  - 指纹：${f.fingerprint}`,
    );
  }
  return lines.join("\n");
}

/** JSON：Finding 字段原样输出，供后续工具消费。 */
export function findingsToJson(findings: Finding[]): string {
  return JSON.stringify(findings, null, 2);
}
