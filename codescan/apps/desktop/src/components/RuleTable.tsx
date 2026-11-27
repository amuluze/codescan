/** 规则目录表：启用开关、元数据与只读来源；选择行由父视图处理。 */
import type { RuleInfo } from "../platform";

type Props = {
  rules: RuleInfo[];
  selectedId: string | null;
  onOpen: (rule: RuleInfo) => void;
  onToggle: (ruleId: string, enabled: boolean) => Promise<void>;
};

export function RuleTable({ rules, selectedId, onOpen, onToggle }: Props) {
  return (
    <table className="rules-table">
      <thead>
        <tr>
          <th>启用</th>
          <th>id</th>
          <th>语言</th>
          <th>维度</th>
          <th>严重级</th>
          <th>来源</th>
          <th>说明</th>
        </tr>
      </thead>
      <tbody data-testid="rules-table">
        {rules.map((rule) => (
          <tr
            key={rule.id}
            className={`${rule.error ? "rule-error-row" : ""} ${
              selectedId === rule.id ? "selected" : ""
            }`}
            onClick={() => onOpen(rule)}
          >
            <td>
              <input
                type="checkbox"
                aria-label={`启用 ${rule.id}`}
                checked={rule.enabled}
                disabled={Boolean(rule.error)}
                onClick={(event) => event.stopPropagation()}
                onChange={(event) => void onToggle(rule.id, event.target.checked)}
              />
            </td>
            <td>{rule.id}</td>
            <td>{rule.language}</td>
            <td>{rule.dimension}</td>
            <td>{rule.severity}</td>
            <td>{rule.origin ?? ""}</td>
            <td>
              {rule.error ? (
                <span className="degraded" title={rule.error}>
                  {rule.message}
                </span>
              ) : (
                rule.message
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
