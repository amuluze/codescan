/** 规则页：内置 + 自定义规则目录、启停、校验错误展示。 */
import { useEffect } from "react";
import { useRulesStore } from "../stores/rulesStore";

export function RulesView() {
  const { rules, customError, loading, load, toggle } = useRulesStore();

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (loading) return <p className="muted">加载规则…</p>;

  return (
    <section className="rules-view" aria-label="规则">
      {customError && (
        <p role="alert" data-testid="custom-rules-error">
          自定义规则加载失败：
          {customError}
        </p>
      )}
      <table className="rules-table">
        <thead>
          <tr>
            <th>启用</th>
            <th>id</th>
            <th>语言</th>
            <th>维度</th>
            <th>严重级</th>
            <th>说明</th>
          </tr>
        </thead>
        <tbody data-testid="rules-table">
          {rules.map((r) => (
            <tr key={r.id}>
              <td>
                <input
                  type="checkbox"
                  aria-label={`启用 ${r.id}`}
                  checked={r.enabled}
                  onChange={(e) => void toggle(r.id, e.target.checked)}
                />
              </td>
              <td>{r.id}</td>
              <td>{r.language}</td>
              <td>{r.dimension}</td>
              <td>{r.severity}</td>
              <td>{r.message}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
