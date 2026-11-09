/** 历史页：扫描记录、基线设置、门禁差异、SARIF 导出。 */
import { useEffect, useState } from "react";
import { useHistoryStore } from "../stores/historyStore";
import { platform } from "../platform";
import { DirectoryPicker } from "./DirectoryPicker";
import type { Finding } from "../types";

export function HistoryView() {
  const {
    scans,
    baseline,
    openScanId,
    openFindings,
    gateNew,
    load,
    openScan,
    setBaseline,
    refreshGate,
  } = useHistoryStore();
  const [error, setError] = useState<string | null>(null);
  const [selectedFingerprint, setSelectedFingerprint] = useState<string | null>(null);
  const [exportPickerOpen, setExportPickerOpen] = useState(false);

  useEffect(() => {
    load().catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const exportSarif = () => {
    if (!openScanId) return;
    setExportPickerOpen(true);
  };

  const exportTo = async (dest: string) => {
    setExportPickerOpen(false);
    if (!openScanId) return;
    try {
      await platform.invoke("export_sarif", {
        scanId: openScanId,
        dest: `${dest}/codescan-${openScanId}.sarif.json`,
      });
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="history-view" aria-label="历史">
      {error && <p role="alert">{error}</p>}
      {exportPickerOpen && (
        <DirectoryPicker
          title="选择 SARIF 导出目录"
          confirmLabel="导出到此目录"
          onSelect={(path) => void exportTo(path)}
          onClose={() => setExportPickerOpen(false)}
        />
      )}
      <p data-testid="baseline-info" className="muted">
        {baseline
          ? `当前基线：${baseline.entries} 条（引擎 ${baseline.engine_version}）`
          : "未设置基线"}
      </p>
      <table className="history-table">
        <thead>
          <tr>
            <th>#</th>
            <th>工作区</th>
            <th>时间</th>
            <th>发现</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody data-testid="history-table">
          {scans.map((s) => (
            <tr key={s.id}>
              <td>{s.id}</td>
              <td>{s.root}</td>
              <td>{new Date(s.started_at_ms).toLocaleString()}</td>
              <td>{s.findings}</td>
              <td>
                <button
                  onClick={() => {
                    setSelectedFingerprint(null);
                    void openScan(s.id);
                  }}
                >
                  查看
                </button>
                <button onClick={() => void setBaseline(s.id)}>设为基线</button>
                <button onClick={() => void refreshGate(s.id)}>门禁差异</button>
                {openScanId === s.id && (
                  <button onClick={() => void exportSarif()}>导出 SARIF</button>
                )}
              </td>
            </tr>
          ))}
          {scans.length === 0 && (
            <tr>
              <td colSpan={5} className="muted">
                暂无扫描记录
              </td>
            </tr>
          )}
        </tbody>
      </table>

      {gateNew && (
        <div data-testid="gate-diff">
          <h3>门禁差异（不在基线内）</h3>
          {gateNew.length === 0 ? (
            <p className="muted">无新增发现</p>
          ) : (
            <ul>
              {gateNew.map((f: Finding) => (
                <li key={f.fingerprint}>
                  {f.path}:{f.span.start.line} {f.rule_id}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {openScanId && (
        <div className="history-findings" data-testid="history-findings">
          <h3>
            扫描 #{openScanId} 的发现（{openFindings.length} 条）
          </h3>
          <div className="findings-body">
            <ul className="finding-list">
              {openFindings.map((f) => (
                <li
                  key={f.fingerprint}
                  className={selectedFingerprint === f.fingerprint ? "selected" : ""}
                  onClick={() => setSelectedFingerprint(f.fingerprint)}
                >
                  <span className={`sev sev-${f.severity}`}>{f.severity}</span>
                  <span className="dim">{f.dimension}</span>
                  <span className="path">
                    {f.path}:{f.span.start.line}
                  </span>
                  <span className="rule">
                    {f.rule_id}
                    {f.degraded && <span className="degraded">degraded</span>}
                  </span>
                </li>
              ))}
              {openFindings.length === 0 && <li className="muted">无发现</li>}
            </ul>
            {(() => {
              const selected = openFindings.find(
                (f) => f.fingerprint === selectedFingerprint,
              );
              if (!selected) return null;
              return (
                <aside className="finding-detail" data-testid="history-finding-detail">
                  <h3>{selected.rule_id}</h3>
                  <p className="muted">
                    {selected.path}:{selected.span.start.line}-
                    {selected.span.end.line}
                  </p>
                  <p>{selected.message}</p>
                  <pre>
                    <code>{selected.match_text}</code>
                  </pre>
                  {selected.flows.length > 0 && (
                    <div data-testid="history-taint-flows">
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
              );
            })()}
          </div>
        </div>
      )}
    </section>
  );
}
