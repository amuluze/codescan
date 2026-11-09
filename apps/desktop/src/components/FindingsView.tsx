/** 发现页：过滤 + 列表 + 详情（命中文本与捕获）。 */
import { useMemo } from "react";
import { filteredFindings, useScanStore } from "../stores/scanStore";
import type { Finding } from "../types";

const DIMENSIONS: Array<"all" | Finding["dimension"]> = [
  "all",
  "security",
  "business-logic",
  "quality",
];
const SEVERITIES: Array<"all" | Finding["severity"]> = ["all", "error", "warning", "info"];

export function FindingsView() {
  const state = useScanStore();
  const { filter, selected, select, setFilter } = state;
  const shown = useMemo(() => filteredFindings(state), [state]);

  return (
    <section className="findings-view" aria-label="发现">
      <div className="row">
        <select
          aria-label="维度过滤"
          value={filter.dimension}
          onChange={(e) => setFilter({ dimension: e.target.value as Finding["dimension"] | "all" })}
        >
          {DIMENSIONS.map((d) => (
            <option key={d} value={d}>
              {d === "all" ? "全部维度" : d}
            </option>
          ))}
        </select>
        <select
          aria-label="严重级过滤"
          value={filter.severity}
          onChange={(e) => setFilter({ severity: e.target.value as Finding["severity"] | "all" })}
        >
          {SEVERITIES.map((s) => (
            <option key={s} value={s}>
              {s === "all" ? "全部严重级" : s}
            </option>
          ))}
        </select>
        <input
          aria-label="搜索"
          placeholder="路径 / 规则 / 消息"
          value={filter.query}
          onChange={(e) => setFilter({ query: e.target.value })}
        />
      </div>

      <div className="findings-body">
        <ul className="finding-list" data-testid="finding-list">
          {shown.map((f) => (
            <li
              key={f.fingerprint}
              className={selected?.fingerprint === f.fingerprint ? "selected" : ""}
              onClick={() => select(f)}
            >
              <span className={`sev sev-${f.severity}`}>{f.severity}</span>
              <span className="dim">{f.dimension}</span>
              <span className="path">
                {f.path}:{f.span.start.line}
              </span>
              <span className="rule">{f.rule_id}</span>
              {f.degraded && <span className="degraded">degraded</span>}
            </li>
          ))}
          {shown.length === 0 && <li className="muted">无匹配发现</li>}
        </ul>

        {selected && (
          <aside className="finding-detail" data-testid="finding-detail">
            <h3>{selected.rule_id}</h3>
            <p className="muted">
              {selected.path}:{selected.span.start.line}-{selected.span.end.line}
            </p>
            <p>{selected.message}</p>
            <pre data-testid="match-text">
              <code>{selected.match_text}</code>
            </pre>
            {selected.captures.length > 0 && (
              <dl>
                {selected.captures.map(([name, text]) => (
                  <div key={name}>
                    <dt>@{name}</dt>
                    <dd>{text}</dd>
                  </div>
                ))}
              </dl>
            )}
          </aside>
        )}
      </div>
    </section>
  );
}
