/** 发现页：过滤 + 列表 + 详情（命中文本与捕获）。 */
import { useMemo, useState } from "react";
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
        <button
          disabled={!scanId || batch === "loading"}
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
        <div className="batch-triage" data-testid="batch-triage">
          <h4>批量分诊（参考）</h4>
          <p className="muted llm-discipline">非确定性参考 · 不参与门禁</p>
          <ul>
            {batch.map((entry) => {
              const finding = shown.find((f) => f.fingerprint === entry.fingerprint);
              return (
                <li key={entry.fingerprint}>
                  <span className="path">
                    {finding
                      ? `${finding.path}:${finding.span.start.line}`
                      : entry.fingerprint}
                  </span>{" "}
                  <span className="dim">
                    {entry.missing ? "missing" : entry.assessment}
                  </span>{" "}
                  <span className="muted">{entry.rationale}</span>
                </li>
              );
            })}
          </ul>
        </div>
      )}

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
            <div className="llm-panel">
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
              <p className="muted llm-discipline">
                非确定性参考 · 不参与门禁与基线；仅保存为确定性规则后才影响后续扫描
              </p>
              {errors[selected.fingerprint] && (
                <p role="alert">{errors[selected.fingerprint]}</p>
              )}
              {streamPending[selected.fingerprint] && (
                <p className="muted">流式分诊接收中…</p>
              )}
              {triageStreams[selected.fingerprint] && (
                <pre data-testid="triage-stream" className="triage-stream">
                  <code>{triageStreams[selected.fingerprint]}</code>
                </pre>
              )}
              {triage[selected.fingerprint] === "loading" && <p className="muted">分诊中…</p>}
              {triage[selected.fingerprint] &&
                triage[selected.fingerprint] !== "loading" && (
                  <div data-testid="triage-result">
                    <h4>分诊建议（参考）</h4>
                    <p>
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
                          <pre className="rule-yaml">
                            <code>{c.yaml}</code>
                          </pre>
                          <div className="row">
                            <button
                              disabled={
                                !c.valid ||
                                regressions[selected.fingerprint] === "loading"
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
                            <label className="muted">
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
                                      期望行：
                                      {r.expected.length > 0
                                        ? r.expected.join(", ")
                                        : "无"}
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
            {selected.flows.length > 0 && (
              <div data-testid="taint-flows">
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
          </aside>
        )}
      </div>
    </section>
  );
}
