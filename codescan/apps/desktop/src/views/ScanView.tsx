/** 扫描页：工作区选择 → 触发/取消 → 阶段与进度 → 结果统计。 */
import { useEffect, useState } from "react";
import { useScanStore } from "../stores/scanStore";
import { DirectoryPicker } from "../components/DirectoryPicker";
import { NoticeDialog } from "../components/NoticeDialog";
import {
  ScanResultCard,
  ScanReviewCard,
  ScanRunCard,
  ScanWorkspaceCard,
  runProgress,
  useScanReview,
} from "./ScanView.parts";

export function ScanView() {
  const {
    phase,
    workspace,
    recentRoots,
    progress,
    error,
    stats,
    events,
    setWorkspace,
    startScan,
    cancelScan,
  } = useScanStore();
  const review = useScanReview();
  const [pickerOpen, setPickerOpen] = useState(false);

  useEffect(() => {
    void useScanStore.getState().initListeners();
    void useScanStore.getState().loadRecentRoots();
    void review.loadProviders();
  }, []);

  const run = runProgress(phase, progress);

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

      <ScanWorkspaceCard
        workspace={workspace}
        recentRoots={recentRoots}
        scanning={run.scanning}
        onChooseDirectory={() => setPickerOpen(true)}
        onSelectRecent={setWorkspace}
        onCancel={() => void cancelScan()}
        onStart={() => void startScan()}
      />
      {phase !== "idle" && phase !== "finished" && (
        <ScanRunCard phase={phase} error={error} {...run} />
      )}
      {phase === "finished" && stats && <ScanResultCard stats={stats} events={events} />}
      <ScanReviewCard model={review} workspace={workspace} />

      {pickerOpen && (
        <DirectoryPicker
          title="选择工作区"
          initialPath={workspace}
          confirmLabel="使用此目录"
          onSelect={(path) => {
            setWorkspace(path);
            review.resetReview();
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
      {review.exportPickerOpen && (
        <DirectoryPicker
          title="选择审查结果导出目录"
          confirmLabel="导出到此目录"
          onSelect={review.exportReviewTo}
          onClose={() => review.setExportPickerOpen(false)}
        />
      )}
      {review.notice && (
        <NoticeDialog
          tone={review.notice.tone}
          title={review.notice.title}
          message={review.notice.message}
          onClose={review.dismiss}
        />
      )}
    </section>
  );
}
