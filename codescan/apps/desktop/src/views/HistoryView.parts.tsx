/**
 * 历史页的区段组件：扫描记录表、门禁差异、选中扫描的下钻面板。
 * 均为 props 驱动、无自有状态；页面骨架、删除确认态与 SARIF 导出见 HistoryView.tsx。
 */
import type { ScanRuleset, ScanSummary } from "../platform";
import type { Finding, ScanEvent } from "../types";
import { formatScanEvent } from "../labels";

/** 扫描记录行的操作：由页面注入，保持各按钮原有的调用方式。 */
type ScanActions = {
  /** 当前打开（下钻）的扫描 id；决定「导出 SARIF」是否可见。 */
  openScanId: number | null;
  /** 待确认删除 / 删除中的扫描 id（内联二次确认）。 */
  confirmingId: number | null;
  deletingId: number | null;
  onOpen: (scanId: number) => void;
  onSetBaseline: (scanId: number) => void;
  onRefreshGate: (scanId: number) => void;
  onExportSarif: () => void;
  onAskDelete: (scanId: number) => void;
  onCancelDelete: () => void;
  onConfirmDelete: (scanId: number) => void;
};

export function HistoryScanTable({
  scans,
  ...actions
}: ScanActions & { scans: ScanSummary[] }) {
  return (
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
          <HistoryScanRow key={s.id} scan={s} actions={actions} />
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
  );
}

function HistoryScanRow({
  scan: s,
  actions,
}: {
  scan: ScanSummary;
  actions: ScanActions;
}) {
  const { openScanId, confirmingId, deletingId } = actions;
  return (
    <tr>
      <td>{s.id}</td>
      <td>{s.root}</td>
      <td>{new Date(s.started_at_ms).toLocaleString()}</td>
      <td>{s.findings}</td>
      <td>
        <div className="table-actions">
          {confirmingId === s.id ? (
            <>
              <button
                className="danger btn-sm"
                disabled={deletingId === s.id}
                onClick={() => void actions.onConfirmDelete(s.id)}
              >
                {deletingId === s.id ? "删除中…" : "确认删除"}
              </button>
              <button
                className="ghost btn-sm"
                disabled={deletingId === s.id}
                onClick={actions.onCancelDelete}
              >
                取消
              </button>
            </>
          ) : (
            <>
              <button className="btn-sm" onClick={() => actions.onOpen(s.id)}>
                查看
              </button>
              <button
                className="ghost btn-sm"
                onClick={() => void actions.onSetBaseline(s.id)}
              >
                设为基线
              </button>
              <button
                className="ghost btn-sm"
                onClick={() => void actions.onRefreshGate(s.id)}
              >
                门禁差异
              </button>
              {openScanId === s.id && (
                <button
                  className="ghost btn-sm"
                  onClick={() => void actions.onExportSarif()}
                >
                  导出 SARIF
                </button>
              )}
              <button
                className="ghost btn-sm danger-text"
                onClick={() => actions.onAskDelete(s.id)}
              >
                删除
              </button>
            </>
          )}
        </div>
      </td>
    </tr>
  );
}

/** 门禁差异：不在基线内的新增发现。 */
export function HistoryGateDiff({ gateNew }: { gateNew: Finding[] }) {
  return (
    <div data-testid="gate-diff">
      <h3>门禁差异（不在基线内）</h3>
      {gateNew.length === 0 ? (
        <p className="muted">无新增发现</p>
      ) : (
        <ul>
          {gateNew.map((f) => (
            <li key={f.fingerprint}>
              {f.path}:{f.span.start.line} {f.rule_id}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

type OpenScanProps = {
  scanId: number;
  findings: Finding[];
  events: ScanEvent[];
  ruleset: ScanRuleset | null;
  selectedFingerprint: string | null;
  onSelect: (fingerprint: string) => void;
};

/** 选中扫描的下钻面板：规则集与扫描事件（折叠）→ 发现列表 ↔ 详情。 */
export function HistoryScanFindings({
  scanId,
  findings,
  events,
  ruleset,
  selectedFingerprint,
  onSelect,
}: OpenScanProps) {
  const selected = findings.find((f) => f.fingerprint === selectedFingerprint);
  return (
    <div className="history-findings" data-testid="history-findings">
      <h3>
        扫描 #{scanId} 的发现（{findings.length} 条）
      </h3>
      {ruleset && (
        <details data-testid="history-ruleset">
          <summary>
            规则集（启用 {ruleset.enabled_rules?.length ?? 0} 条，规则包{" "}
            {ruleset.packs?.length ?? 0} 个）
          </summary>
          {ruleset.packs && ruleset.packs.length > 0 && (
            <ul>
              {ruleset.packs.map((pack) => (
                <li key={`${pack.id}@${pack.version}`}>
                  {pack.id}@{pack.version}
                </li>
              ))}
            </ul>
          )}
        </details>
      )}
      {events.length > 0 && (
        <details data-testid="history-events">
          <summary>扫描事件（{events.length}）</summary>
          <ul>
            {events.map((event, index) => (
              <li key={`${event.kind}-${index}`}>{formatScanEvent(event)}</li>
            ))}
          </ul>
        </details>
      )}
      <div className="findings-body">
        <ul className="finding-list">
          {findings.map((f) => (
            <li
              key={f.fingerprint}
              className={selectedFingerprint === f.fingerprint ? "selected" : ""}
              onClick={() => onSelect(f.fingerprint)}
            >
              <span className={`sev-chip sev-${f.severity}`}>{f.severity}</span>
              <span className="dim-badge">{f.dimension}</span>
              <span className="path">
                {f.path}:{f.span.start.line}
              </span>
              <span className="rule">
                {f.rule_id}
                {f.degraded && <span className="degraded">degraded</span>}
              </span>
            </li>
          ))}
          {findings.length === 0 && (
            <li className="empty-row">
              <div className="empty-state">
                <strong>无发现</strong>
              </div>
            </li>
          )}
        </ul>
        {selected && (
          <aside className="finding-detail" data-testid="history-finding-detail">
            <h3>{selected.rule_id}</h3>
            <p className="muted">
              {selected.path}:{selected.span.start.line}-
              {selected.span.end.line}
            </p>
            <p>{selected.message}</p>
            <pre className="code-block">
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
        )}
      </div>
    </div>
  );
}
