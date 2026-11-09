/** 发现页：摘要 + 过滤 + 表格化列表 + 详情（证据 / 污点链 / LLM 辅助）。 */
import { useEffect, useMemo, useState } from "react";
import { filteredFindings, useScanStore } from "../stores/scanStore";
import { useLlmStore } from "../stores/llmStore";
import type { Finding } from "../types";

const DIMENSIONS: Array<"all" | Finding["dimension"]> = [
  "all",
  "security",
  "business-logic",
  "quality",
];
const SEVERITIES: Array<"all" | Finding["severity"]> = ["all", "error", "warning", "info"];

const SEVERITY_LABEL: Record<Finding["severity"], string> = {
  error: "错误",
  warning: "警告",
  info: "提示",
};
const DIMENSION_LABEL: Record<Finding["dimension"], string> = {
  security: "安全",
  "business-logic": "业务逻辑",
  quality: "质量",
};

export function FindingsView() {
  const state = useScanStore();
  const { filter, selected, select, setFilter } = state;
  const shown = useMemo(() => filteredFindings(state), [state]);
  const {
    triage,
    triageStreams,
    streamPending,
    candidates,
    regressions,
    batch,
    batchError,
    errors,
    runTriage,
    runTriageStream,
    runBatchTriage,
    generateRule,
    runRegression,
    saveRule,
  } = useLlmStore();
  const scanId = state.lastScanId;
  const [saveFixture, setSaveFixture] = useState(false);

  const counts = useMemo(() => {
    const base = { error: 0, warning: 0, info: 0 };
    for (const f of state.findings) {
      base[f.severity] += 1;
    }
    return base;
  }, [state.findings]);

  // 过滤后保持选中项有效：选中项被过滤掉时自动落到第一条。
  useEffect(() => {
    if (shown.length === 0) {
      if (selected) select(null);
      return;
    }
    if (!selected || !shown.some((f) => f.fingerprint === selected.fingerprint)) {
      select(shown[0]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, selected?.fingerprint]);

  const hasFilter =
    filter.dimension !== "all" || filter.severity !== "all" || filter.query.trim() !== "";

  return (
    <section className="page findings-view" aria-label="发现">
      <header className="page-head">
        <div>
          <h2>发现</h2>
          <p className="page-sub">
            结果由确定性规则与指纹排序；点击条目查看证据、污点链与 LLM 辅助。
          </p>
        </div>
        <div className="summary-bar" data-testid="finding-summary">
          <span className="count-chip">共 {state.findings.length}</span>
          <span className="count-chip sev-error">错误 {counts.error}</span>
          <span className="count-chip sev-warning">警告 {counts.warning}</span>
          <span className="count-chip sev-info">提示 {counts.info}</span>
        </div>
      </header>

      <div className="toolbar">
        <select
          aria-label="维度过滤"
          value={filter.dimension}
          onChange={(e) => setFilter({ dimension: e.target.value as Finding["dimension"] | "all" })}
        >
          {DIMENSIONS.map((d) => (
            <option key={d} value={d}>
              {d === "all" ? "全部维度" : DIMENSION_LABEL[d]}
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
              {s === "all" ? "全部严重级" : SEVERITY_LABEL[s]}
            </option>
          ))}
        </select>
        <input
          aria-label="搜索"
          className="search-input"
          placeholder="搜索路径 / 规则 / 消息"
          value={filter.query}
          onChange={(e) => setFilter({ query: e.target.value })}
        />
        {hasFilter && (
          <button
            className="ghost"
            onClick={() => setFilter({ dimension: "all", severity: "all", query: "" })}
          >
            清除过滤
          </button>
        )}
        <div className="toolbar-spacer" />
        <span className="toolbar-count muted">显示 {shown.length} 条</span>
        <button
          disabled={!scanId || batch === "loading" || shown.length === 0}
          onClick={() =>
            scanId &&
            void runBatchTriage(
              scanId,
              shown.slice(0, 20).map((f) => f.fingerprint),
            )
          }
        >
          批量分诊
        </button>
      </div>

      {batchError && <p role="alert">{batchError}</p>}
      {batch === "loading" && <p className="muted">批量分诊中…</p>}
      {batch && batch !== "loading" && (
        <div className="card batch-triage" data-testid="batch-triage">
          <header className="card-head">
            <h3>批量分诊（参考）</h3>
            <span className="discipline-tag">非确定性参考 · 不参与门禁</span>
          </header>
          <ul>
            {batch.map((entry) => {
              const finding = shown.find((f) => f.fingerprint === entry.fingerprint);
              return (
                <li key={entry.fingerprint}>
                  <span className="path">
                    {finding ? `${finding.path}:${finding.span.start.line}` : entry.fingerprint}
                  </span>{" "}
                  <span className="assessment">{entry.missing ? "missing" : entry.assessment}</span>{" "}
                  <span className="muted">{entry.rationale}</span>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      <div className="findings-body">
        <div className="finding-table">
          <div className="finding-table-head" aria-hidden="true">
            <span>严重级</span>
            <span>维度</span>
            <span>位置</span>
            <span>规则</span>
          </div>
          <ul className="finding-list" data-testid="finding-list">
            {shown.map((f) => (
              <li
                key={f.fingerprint}
                className={selected?.fingerprint === f.fingerprint ? "selected" : ""}
                onClick={() => select(f)}
              >
                <span className={`sev-chip sev-${f.severity}`}>
                  {SEVERITY_LABEL[f.severity]}
                </span>
                <span className="dim-badge">{DIMENSION_LABEL[f.dimension]}</span>
                <span className="path">
                  {f.path}:{f.span.start.line}
                </span>
                <span className="rule">
                  {f.rule_id}
                  {f.degraded && <span className="degraded">degraded</span>}
                </span>
              </li>
            ))}
            {shown.length === 0 && (
              <li className="empty-row">
                <div className="empty-state">
                  <strong>无匹配发现</strong>
                  <p className="muted">
                    {state.findings.length === 0
                      ? "先在「扫描」页选择工作区并运行一次扫描。"
                      : "当前过滤条件下没有结果，尝试放宽维度 / 严重级或清空搜索。"}
                  </p>
                </div>
              </li>
            )}
          </ul>
        </div>

        {selected ? (
          <aside className="finding-detail" data-testid="finding-detail">
            <header className="detail-head">
              <div>
                <h3>{selected.rule_id}</h3>
                <p className="muted">
                  {selected.path}:{selected.span.start.line}-{selected.span.end.line}
                </p>
              </div>
              <div className="detail-tags">
                <span className={`sev-chip sev-${selected.severity}`}>
                  {SEVERITY_LABEL[selected.severity]}
                </span>
                <span className="dim-badge">{DIMENSION_LABEL[selected.dimension]}</span>
              </div>
            </header>
            <p className="detail-message">{selected.message}</p>
            <pre data-testid="match-text" className="code-block">
              <code>{selected.match_text}</code>
            </pre>
            {selected.captures.length > 0 && (
              <dl className="captures">
                {selected.captures.map(([name, text]) => (
                  <div key={name}>
                    <dt>@{name}</dt>
                    <dd>{text}</dd>
                  </div>
                ))}
              </dl>
            )}
            {selected.flows.length > 0 && (
              <div className="detail-section" data-testid="taint-flows">
                <h4>污点来源（过程内近似）</h4>
                <ul>
                  {selected.flows.map((p) => (
                    <li key={`${p.line}:${p.column}`}>
                      第 {p.line} 行第 {p.column} 列 → 当前 sink
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div className="detail-section llm-panel">
              <h4>LLM 辅助</h4>
              <div className="row">
                <button
                  disabled={!scanId || triage[selected.fingerprint] === "loading"}
                  onClick={() => scanId && void runTriage(scanId, selected.fingerprint)}
                >
                  LLM 分诊
                </button>
                <button
                  disabled={!scanId || streamPending[selected.fingerprint]}
                  onClick={() => scanId && void runTriageStream(scanId, selected.fingerprint)}
                >
                  流式分诊
                </button>
                <button
                  disabled={!scanId || candidates[selected.fingerprint] === "loading"}
                  onClick={() => scanId && void generateRule(scanId, selected.fingerprint)}
                >
                  生成候选规则
                </button>
              </div>
              <p className="discipline-tag">非确定性参考 · 不参与门禁与基线；仅保存为确定性规则后才影响后续扫描</p>
              {errors[selected.fingerprint] && (
                <p role="alert">{errors[selected.fingerprint]}</p>
              )}
              {streamPending[selected.fingerprint] && (
                <p className="muted">流式分诊接收中…</p>
              )}
              {triageStreams[selected.fingerprint] && (
                <pre data-testid="triage-stream" className="code-block triage-stream">
                  <code>{triageStreams[selected.fingerprint]}</code>
                </pre>
              )}
              {triage[selected.fingerprint] === "loading" && <p className="muted">分诊中…</p>}
              {triage[selected.fingerprint] && triage[selected.fingerprint] !== "loading" && (
                <div data-testid="triage-result">
                  <h4>分诊建议（参考）</h4>
                  <p className="assessment">
                    {(() => {
                      const t = triage[selected.fingerprint];
                      return t && t !== "loading" ? t.assessment : "";
                    })()}
                  </p>
                  <p className="muted">
                    {(() => {
                      const t = triage[selected.fingerprint];
                      return t && t !== "loading" ? t.rationale : "";
                    })()}
                  </p>
                </div>
              )}
              {candidates[selected.fingerprint] === "loading" && (
                <p className="muted">生成候选规则中…</p>
              )}
              {candidates[selected.fingerprint] &&
                candidates[selected.fingerprint] !== "loading" && (
                  <div data-testid="candidate-rule">
                    {(() => {
                      const c = candidates[selected.fingerprint];
                      if (!c || c === "loading") return null;
                      return (
                        <>
                          <h4>候选规则（{c.valid ? "校验通过" : "校验未通过"}）</h4>
                          {!c.valid && (
                            <ul className="errors">
                              {c.errors.map((e) => (
                                <li key={e}>{e}</li>
                              ))}
                            </ul>
                          )}
                          <pre className="code-block rule-yaml">
                            <code>{c.yaml}</code>
                          </pre>
                          <div className="row">
                            <button
                              disabled={
                                !c.valid || regressions[selected.fingerprint] === "loading"
                              }
                              onClick={() =>
                                scanId &&
                                void runRegression(
                                  scanId,
                                  selected.fingerprint,
                                  c.yaml,
                                  saveFixture,
                                )
                              }
                            >
                              运行回归验证
                            </button>
                            <label className="inline-check muted">
                              <input
                                type="checkbox"
                                aria-label="保存回归夹具"
                                checked={saveFixture}
                                onChange={(e) => setSaveFixture(e.target.checked)}
                              />
                              保存夹具
                            </label>
                          </div>
                          {regressions[selected.fingerprint] === "loading" && (
                            <p className="muted">隔离回归中…</p>
                          )}
                          {(() => {
                            const r = regressions[selected.fingerprint];
                            if (!r || r === "loading") return null;
                            return (
                              <div data-testid="regression-result">
                                <h4>隔离回归：{r.pass ? "通过" : "未通过"}</h4>
                                {!r.valid && r.errors.length > 0 && (
                                  <ul className="errors">
                                    {r.errors.map((e) => (
                                      <li key={e}>{e}</li>
                                    ))}
                                  </ul>
                                )}
                                {r.valid && (
                                  <>
                                    <p>
                                      期望行：{r.expected.length > 0 ? r.expected.join(", ") : "无"}
                                    </p>
                                    <p>
                                      实际行：
                                      {r.actual.length > 0
                                        ? r.actual
                                            .map((a) => `${a.line}（${a.message}）`)
                                            .join(", ")
                                        : "无"}
                                    </p>
                                    {r.failures.length > 0 && (
                                      <ul className="errors">
                                        {r.failures.map((f, idx) => (
                                          <li key={`${idx}-${f}`}>{f}</li>
                                        ))}
                                      </ul>
                                    )}
                                    {r.saved_paths.length > 0 && (
                                      <p className="muted">
                                        夹具已保存：{r.saved_paths.join(", ")}
                                      </p>
                                    )}
                                  </>
                                )}
                              </div>
                            );
                          })()}
                          <button
                            className="primary"
                            disabled={!c.valid}
                            onClick={() => void saveRule(selected.fingerprint, c.yaml)}
                          >
                            保存为自定义规则
                          </button>
                        </>
                      );
                    })()}
                  </div>
                )}
            </div>
          </aside>
        ) : (
          <aside className="finding-detail detail-empty">
            <p className="muted">选择左侧任意发现查看详情。</p>
          </aside>
        )}
      </div>
    </section>
  );
}
