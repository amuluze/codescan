/** 历史页：扫描记录、基线设置、门禁差异、SARIF 导出。 */
import { useEffect, useState } from "react";
import { useHistoryStore } from "../stores/historyStore";
import { api } from "../platform";
import { DirectoryPicker } from "../components/DirectoryPicker";
import {
  HistoryGateDiff,
  HistoryScanFindings,
  HistoryScanTable,
} from "./HistoryView.parts";

export function HistoryView() {
  const {
    scans,
    baseline,
    openScanId,
    openFindings,
    openEvents,
    openRuleset,
    gateNew,
    load,
    openScan,
    setBaseline,
    refreshGate,
    deleteScan,
  } = useHistoryStore();
  const [error, setError] = useState<string | null>(null);
  const [selectedFingerprint, setSelectedFingerprint] = useState<string | null>(null);
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  /** 待确认删除的扫描 id（二次确认，避免误删）。 */
  const [confirmingId, setConfirmingId] = useState<number | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);

  const removeScan = async (scanId: number) => {
    setDeletingId(scanId);
    setError(null);
    try {
      await deleteScan(scanId);
      setConfirmingId(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setDeletingId(null);
    }
  };

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
      await api.exportSarif(openScanId, dest);
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
      <HistoryScanTable
        scans={scans}
        openScanId={openScanId}
        confirmingId={confirmingId}
        deletingId={deletingId}
        onOpen={(scanId) => {
          setSelectedFingerprint(null);
          void openScan(scanId);
        }}
        onSetBaseline={(scanId) => void setBaseline(scanId)}
        onRefreshGate={(scanId) => void refreshGate(scanId)}
        onExportSarif={exportSarif}
        onAskDelete={setConfirmingId}
        onCancelDelete={() => setConfirmingId(null)}
        onConfirmDelete={removeScan}
      />
      {gateNew && <HistoryGateDiff gateNew={gateNew} />}
      {openScanId && (
        <HistoryScanFindings
          scanId={openScanId}
          findings={openFindings}
          events={openEvents}
          ruleset={openRuleset}
          selectedFingerprint={selectedFingerprint}
          onSelect={setSelectedFingerprint}
        />
      )}
    </section>
  );
}
