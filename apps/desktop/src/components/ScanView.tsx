/** 扫描页：工作区选择 → 触发/取消 → 进度 → 概览。 */
import { useEffect } from "react";
import { useScanStore } from "../stores/scanStore";

export function ScanView() {
  const { phase, workspace, progress, error, stats, pickWorkspace, startScan, cancelScan } =
    useScanStore();

  useEffect(() => {
    void useScanStore.getState().initListeners();
  }, []);

  const scanning = phase === "scanning";
  const progressText =
    progress?.stage === "scanning"
      ? `扫描中 ${progress.done ?? 0}/${progress.total ?? 0}`
      : progress?.stage === "walking"
        ? "收集文件…"
        : progress?.stage === "finalizing"
          ? "汇总中…"
          : "";

  return (
    <section className="scan-view" aria-label="扫描">
      <div className="row">
        <button onClick={() => void pickWorkspace()}>选择工作区</button>
        <span className="workspace-path" data-testid="workspace-path">
          {workspace ?? "未选择"}
        </span>
        <button onClick={() => void startScan()} disabled={!workspace || scanning}>
          开始扫描
        </button>
        <button onClick={() => void cancelScan()} disabled={!scanning}>
          取消
        </button>
      </div>

      {progressText && (
        <p role="status" data-testid="progress">
          {progressText}
        </p>
      )}
      {phase === "cancelled" && <p role="status">扫描已取消（结果作废）</p>}
      {phase === "error" && <p role="alert">{error}</p>}

      {phase === "finished" && stats && (
        <div className="stats-grid" data-testid="stats">
          <span>发现 {stats.findings}</span>
          <span>扫描文件 {stats.files_scanned}</span>
          <span>忽略 {stats.files_ignored}</span>
          <span>未知语言 {stats.files_unknown_language}</span>
        </div>
      )}
    </section>
  );
}
