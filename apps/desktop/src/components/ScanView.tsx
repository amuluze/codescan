/** 扫描页：工作区选择 → 触发/取消 → 阶段与进度 → 结果统计。 */
import { useEffect, useState } from "react";
import { useScanStore } from "../stores/scanStore";
import { DirectoryPicker } from "./DirectoryPicker";

const PHASE_LABEL: Record<string, string> = {
  idle: "空闲",
  scanning: "扫描中",
  finished: "已完成",
  cancelled: "已取消",
  error: "失败",
};

export function ScanView() {
  const { phase, workspace, progress, error, stats, setWorkspace, startScan, cancelScan } =
    useScanStore();
  const [pickerOpen, setPickerOpen] = useState(false);

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
  const total = progress?.total ?? 0;
  const done = progress?.done ?? 0;
  const percent =
    scanning && progress?.stage === "scanning" && total > 0
      ? Math.min(100, Math.round((done / total) * 100))
      : null;

  return (
    <section className="page scan-view" aria-label="扫描">
      <header className="page-head">
        <div>
          <h2>扫描</h2>
          <p className="page-sub">
            对本地工作区运行确定性规则；扫描只读工作区，结果写入本地历史并可在「发现」页过滤查看。
          </p>
        </div>
      </header>

      <section className="card workspace-card">
        <div className="workspace-row">
          <button onClick={() => setPickerOpen(true)}>选择工作区</button>
          <span
            className={`workspace-path ${workspace ? "" : "muted"}`}
            data-testid="workspace-path"
            title={workspace ?? undefined}
          >
            {workspace ?? "未选择"}
          </span>
          <div className="workspace-actions">
            <button className="ghost" onClick={() => void cancelScan()} disabled={!scanning}>
              取消
            </button>
            <button
              className="primary"
              onClick={() => void startScan()}
              disabled={!workspace || scanning}
            >
              开始扫描
            </button>
          </div>
        </div>
        {!workspace && (
          <p className="card-sub">尚未选择工作区。点击「选择工作区」浏览本地目录后开始。</p>
        )}
      </section>

      {phase !== "idle" && phase !== "finished" && (
        <section className="card run-card">
          <div className="run-head">
            <span className={`run-status run-${phase}`}>{PHASE_LABEL[phase] ?? phase}</span>
            {progressText && (
              <span role="status" data-testid="progress" className="run-progress-text">
                {progressText}
              </span>
            )}
            {percent !== null && <span className="run-percent muted">{percent}%</span>}
          </div>
          {scanning && (
            <div
              className={`progress-track ${percent === null ? "indeterminate" : ""}`}
              aria-hidden="true"
            >
              <div
                className="progress-fill"
                style={percent === null ? undefined : { width: `${percent}%` }}
              />
            </div>
          )}
          {phase === "cancelled" && (
            <p role="status" className="muted run-note">
              扫描已取消（结果作废）
            </p>
          )}
          {phase === "error" && (
            <p role="alert" className="run-error">
              {error}
            </p>
          )}
        </section>
      )}

      {phase === "finished" && stats && (
        <section className="card result-card">
          <header className="card-head">
            <div>
              <h3>扫描结果</h3>
              <p className="card-sub">前往「发现」页查看证据、污点链与 LLM 辅助分诊。</p>
            </div>
          </header>
          <div className="stats-grid" data-testid="stats">
            <div className="stat">
              <strong>{stats.findings}</strong>
              <span>发现</span>
            </div>
            <div className="stat">
              <strong>{stats.files_scanned}</strong>
              <span>扫描文件</span>
            </div>
            <div className="stat">
              <strong>{stats.files_ignored}</strong>
              <span>已忽略</span>
            </div>
            <div className="stat">
              <strong>{stats.files_unknown_language}</strong>
              <span>未知语言</span>
            </div>
          </div>
        </section>
      )}

      {pickerOpen && (
        <DirectoryPicker
          title="选择工作区"
          initialPath={workspace}
          confirmLabel="使用此目录"
          onSelect={(path) => {
            setWorkspace(path);
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </section>
  );
}
