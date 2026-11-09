/** 历史页：扫描记录、基线设置、门禁差异、SARIF 导出。 */
import { useEffect, useState } from "react";
import { useHistoryStore } from "../stores/historyStore";
import { platform } from "../platform";
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

  useEffect(() => {
    load().catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const exportSarif = async () => {
    if (!openScanId) return;
    const dest = await platform.pickFolder();
    if (!dest) return;
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
              <td>{new Date(s.startedAtMs).toLocaleString()}</td>
              <td>{s.findings}</td>
              <td>
                <button onClick={() => void openScan(s.id)}>查看</button>
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

      {openScanId && gateNew === null && (
        <p className="muted">已打开扫描 #{openScanId}（{openFindings.length} 条发现）</p>
      )}
    </section>
  );
}
