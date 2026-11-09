/** 扫描页：工作区选择 → 触发/取消 → 阶段与进度 → 结果统计。 */
import { useEffect, useState } from "react";
import { useScanStore } from "../stores/scanStore";
import { useLlmStore } from "../stores/llmStore";
import { CONFIDENCE_LABEL, DIMENSION_LABEL, SEVERITY_LABEL } from "../labels";
import { CopyButton } from "./CopyButton";
import { DirectoryPicker } from "./DirectoryPicker";

/** 与后端 llm::REVIEW_MAX_FILES 一致（仅用于文案与选项上限）。 */
const REVIEW_MAX_FILES = 24;

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
  const { review, reviewError, reviewProgress, runReview, resetReview } = useLlmStore();
  const [reviewLimit, setReviewLimit] = useState(REVIEW_MAX_FILES);
  const reviewResult = review && review !== "loading" ? review : null;
  const reviewComplete =
    reviewResult !== null && reviewResult.next_offset >= reviewResult.candidate_files;
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

      <section className="card review-card">
        <header className="card-head">
          <div>
            <h3>LLM 源码审查（参考）</h3>
            <p className="card-sub">
              每次最多 {reviewLimit} 个可审查源码文件（合计 ≤128 KiB）发送到已配置 Provider，
              覆盖安全、业务逻辑与代码质量三个维度；可分次「继续审查」直至覆盖全部候选。
              源码被视为不可信数据；结果仅作参考，不写入发现、基线或门禁。
            </p>
          </div>
          <span className="discipline-tag">非确定性参考 · 不参与门禁</span>
        </header>
        <div className="row review-actions">
          <button
            className="primary"
            onClick={() =>
              workspace &&
              void runReview(workspace, reviewLimit, reviewResult?.next_offset ?? 0)
            }
            disabled={!workspace || review === "loading" || reviewComplete}
          >
            {review === "loading"
              ? reviewProgress
                ? `审查中 ${reviewProgress.done}/${reviewProgress.total} 批…`
                : "审查中…"
              : reviewComplete
                ? "已审查全部候选"
                : reviewResult
                  ? "继续审查"
                  : "开始审查"}
          </button>
          {reviewResult && reviewResult.files_reviewed > 0 && (
            <button className="ghost" onClick={() => resetReview()} disabled={review === "loading"}>
              重置进度
            </button>
          )}
          <label className="inline-check muted">
            文件数
            <select
              aria-label="审查文件上限"
              value={reviewLimit}
              onChange={(e) => setReviewLimit(Number(e.target.value))}
              disabled={review === "loading"}
            >
              <option value={12}>12</option>
              <option value={24}>24</option>
            </select>
          </label>
          {review && review !== "loading" && (
            <span className="muted review-summary">
              已审查 {review.files_reviewed} / {review.candidate_files} 个可审查文件 ·{" "}
              {review.suggestions.length} 条建议
              {review.dropped > 0 && ` · 过滤 ${review.dropped} 条无效`}
            </span>
          )}
        </div>
        {reviewResult && !reviewComplete && (
          <p className="muted review-budget">
            还有 {reviewResult.candidate_files - reviewResult.next_offset} 个候选未审查，
            可点「继续审查」逐页覆盖；也可增大文件数上限减少次数。
          </p>
        )}
        {reviewError && (
          <p role="alert" className="run-error">
            {reviewError}
          </p>
        )}
        {review && review !== "loading" &&
          (review.suggestions.length === 0 ? (
            <p className="muted review-empty">未发现问题（参考）；这不代表代码安全。</p>
          ) : (
            <ul className="review-list" data-testid="review-suggestions">
              {review.suggestions.map((s, index) => (
                <li key={`${s.path}:${s.line}:${s.title}`}>
                  <div className="review-head">
                    <span className={`sev-chip sev-${s.severity}`}>
                      {SEVERITY_LABEL[s.severity]}
                    </span>
                    <span className="dim-badge">{DIMENSION_LABEL[s.category]}</span>
                    <span className="path">
                      {s.path}:{s.line}
                    </span>
                    <span className="review-head-actions">
                      <CopyButton
                        label={`复制审查结果第 ${index + 1} 条`}
                        text={[
                          `${SEVERITY_LABEL[s.severity]}｜${DIMENSION_LABEL[s.category]}｜${s.path}:${s.line}`,
                          s.title,
                          s.detail,
                          `置信度：${CONFIDENCE_LABEL[s.confidence]}`,
                        ]
                          .filter((line) => line.trim() !== "")
                          .join("\n")}
                      />
                      <span className="confidence-chip">
                        置信度 {CONFIDENCE_LABEL[s.confidence]}
                      </span>
                    </span>
                  </div>
                  <p className="review-title">{s.title}</p>
                  {s.detail && <p className="muted review-detail">{s.detail}</p>}
                </li>
              ))}
            </ul>
          ))}

      </section>

      {pickerOpen && (
        <DirectoryPicker
          title="选择工作区"
          initialPath={workspace}
          confirmLabel="使用此目录"
          onSelect={(path) => {
            setWorkspace(path);
            resetReview();
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </section>
  );
}
