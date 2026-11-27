/**
 * 扫描页的构建块：LLM 审查页面态 hook、进度派生与各区段展示组件。
 * 展示组件均由 props 驱动、无自有状态；页面骨架与 store 接线见 ScanView.tsx。
 */
import { useMemo, useState, type Dispatch, type SetStateAction } from "react";
import { useLlmStore } from "../stores/llmStore";
import type { ScanPhase } from "../stores/scanStore";
import {
  CONFIDENCE_LABEL,
  DIMENSION_LABEL,
  SEVERITY_LABEL,
  formatScanEvent,
} from "../labels";
import { CopyButton } from "../components/CopyButton";
import { useReportExport, type ExportNotice } from "../useReportExport";
import {
  EMPTY_REVIEW_FILTER,
  VERIFICATION_LABEL,
  filterSuggestions,
  hasReviewFilter,
  suggestionsToJson,
  suggestionsToMarkdown,
  type ReviewFilter,
} from "../reviewExport";
import type {
  ProgressPayload,
  ProvidersInfo,
  ReviewResult,
  ReviewSuggestion,
} from "../platform";
import type { ScanEvent, ScanStats } from "../types";

/** 与后端 llm::REVIEW_MAX_FILES 一致（仅用于文案与选项上限）。 */
const REVIEW_MAX_FILES = 24;

const PHASE_LABEL: Record<string, string> = {
  idle: "空闲",
  scanning: "扫描中",
  finished: "已完成",
  cancelled: "已取消",
  error: "失败",
};

type ReviewProgress = { done: number; total: number };

/** LLM 源码审查的页面态：文件数上限、独立复核、建议筛选、导出，及其派生计数。 */
type ScanReviewModel = {
  review: ReviewResult | null;
  reviewRunning: boolean;
  reviewError: string | null;
  /** 分批审查进度（批次维度）。 */
  reviewProgress: ReviewProgress | null;
  /** 已覆盖全部候选文件。 */
  reviewComplete: boolean;
  suggestions: ReviewSuggestion[];
  /** 当前筛选下的建议。 */
  shownSuggestions: ReviewSuggestion[];
  severityCounts: { error: number; warning: number; info: number };
  verificationCounts: { confirmed: number; needs: number };
  reviewLimit: number;
  setReviewLimit: Dispatch<SetStateAction<number>>;
  verifyReview: boolean;
  setVerifyReview: Dispatch<SetStateAction<boolean>>;
  reviewFilter: ReviewFilter;
  setReviewFilter: Dispatch<SetStateAction<ReviewFilter>>;
  providers: ProvidersInfo | null;
  /** 审查显式选择的 Profile；null = 活动 Profile。 */
  reviewProviderId: string | null;
  setReviewProvider: (id: string | null) => void;
  exportPickerOpen: boolean;
  setExportPickerOpen: Dispatch<SetStateAction<boolean>>;
  exporting: boolean;
  notice: ExportNotice | null;
  dismiss: () => void;
  /** 导出当前筛选下的建议（Markdown + JSON 双文件）。 */
  exportReviewTo: (dest: string) => void;
  runReview: (
    root: string,
    maxFiles?: number,
    offset?: number,
    providerId?: string | null,
    verify?: boolean,
  ) => Promise<void>;
  resetReview: () => void;
  loadProviders: () => Promise<void>;
};

export function useScanReview(): ScanReviewModel {
  const {
    review,
    reviewRunning,
    reviewError,
    reviewProgress,
    runReview,
    resetReview,
    providers,
    loadProviders,
    reviewProviderId,
    setReviewProvider,
  } = useLlmStore();
  const [reviewLimit, setReviewLimit] = useState(REVIEW_MAX_FILES);
  const [verifyReview, setVerifyReview] = useState(true);
  const [reviewFilter, setReviewFilter] = useState<ReviewFilter>(EMPTY_REVIEW_FILTER);
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  const { exporting, notice, run: runExport, dismiss } = useReportExport();
  const reviewResult = review;
  const reviewComplete =
    reviewResult !== null && reviewResult.next_offset >= reviewResult.candidate_files;
  const suggestions = reviewResult?.suggestions ?? [];
  const shownSuggestions = filterSuggestions(suggestions, reviewFilter);
  /** 审查结果按严重级的计数（统计卡片用）。 */
  const severityCounts = useMemo(() => {
    const base = { error: 0, warning: 0, info: 0 };
    for (const s of suggestions) {
      base[s.severity] += 1;
    }
    return base;
  }, [suggestions]);
  /** 独立复核结论计数（摘要用）。 */
  const verificationCounts = useMemo(() => {
    const base = { confirmed: 0, needs: 0 };
    for (const s of suggestions) {
      if (s.verification === "confirmed") base.confirmed += 1;
      if (s.verification === "needs_validation") base.needs += 1;
    }
    return base;
  }, [suggestions]);

  /** 导出当前筛选下的建议（Markdown + JSON 双文件），目录由 DirectoryPicker 选定。 */
  const exportReviewTo = (dest: string) => {
    setExportPickerOpen(false);
    void runExport(
      dest,
      "codescan-review",
      suggestionsToMarkdown(shownSuggestions),
      suggestionsToJson(shownSuggestions),
    );
  };

  return {
    review,
    reviewRunning,
    reviewError,
    reviewProgress,
    reviewComplete,
    suggestions,
    shownSuggestions,
    severityCounts,
    verificationCounts,
    reviewLimit,
    setReviewLimit,
    verifyReview,
    setVerifyReview,
    reviewFilter,
    setReviewFilter,
    providers,
    reviewProviderId,
    setReviewProvider,
    exportPickerOpen,
    setExportPickerOpen,
    exporting,
    notice,
    dismiss,
    exportReviewTo,
    runReview,
    resetReview,
    loadProviders,
  };
}

/** 运行卡的进度视图（由扫描阶段与进度载荷派生）。 */
type ScanRunProgress = {
  scanning: boolean;
  progressText: string;
  /** scanning 阶段且已知总量时的百分比；否则为 null（不确定进度）。 */
  percent: number | null;
};

export function runProgress(
  phase: ScanPhase,
  progress: ProgressPayload | null,
): ScanRunProgress {
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
  return { scanning, progressText, percent };
}

/** 工作区卡：选择 / 历史快捷选择 / 当前路径，以及取消与开始扫描。 */
export function ScanWorkspaceCard({
  workspace,
  recentRoots,
  scanning,
  onChooseDirectory,
  onSelectRecent,
  onCancel,
  onStart,
}: {
  workspace: string | null;
  recentRoots: string[];
  scanning: boolean;
  onChooseDirectory: () => void;
  onSelectRecent: (root: string) => void;
  onCancel: () => void;
  onStart: () => void;
}) {
  return (
    <section className="card workspace-card">
      <div className="workspace-row">
        <button onClick={onChooseDirectory}>选择工作区</button>
        {recentRoots.length > 0 && (
          <select
            className="workspace-recent"
            data-testid="recent-workspace"
            aria-label="从历史选择工作区"
            value=""
            onChange={(e) => {
              const root = e.target.value;
              if (root) onSelectRecent(root);
            }}
          >
            <option value="">历史工作区…</option>
            {recentRoots.map((root) => (
              <option key={root} value={root}>
                {root}
              </option>
            ))}
          </select>
        )}
        <span
          className={`workspace-path ${workspace ? "" : "muted"}`}
          data-testid="workspace-path"
          title={workspace ?? undefined}
        >
          {workspace ?? "未选择"}
        </span>
        <div className="workspace-actions">
          <button className="ghost" onClick={onCancel} disabled={!scanning}>
            取消
          </button>
          <button className="primary" onClick={onStart} disabled={!workspace || scanning}>
            开始扫描
          </button>
        </div>
      </div>
      {!workspace && (
        <p className="card-sub">
          {recentRoots.length > 0
            ? "尚未选择工作区。从历史快速选择，或点击「选择工作区」浏览本地目录。"
            : "尚未选择工作区。点击「选择工作区」浏览本地目录后开始。"}
        </p>
      )}
    </section>
  );
}

/** 运行卡：阶段状态文字、进度文案与百分比、进度条，以及取消 / 失败提示。 */
export function ScanRunCard({
  phase,
  error,
  scanning,
  progressText,
  percent,
}: { phase: ScanPhase; error: string | null } & ScanRunProgress) {
  return (
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
  );
}

/** 扫描结果卡：统计网格（发现 / 扫描文件 / 已忽略 / 未知语言）与扫描事件。 */
export function ScanResultCard({ stats, events }: { stats: ScanStats; events: ScanEvent[] }) {
  return (
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
      {events.length > 0 && (
        <details className="detail-section" data-testid="scan-events">
          <summary>扫描事件（{events.length}）</summary>
          <ul>
            {events.map((event, index) => (
              <li key={`${event.kind}-${index}`}>{formatScanEvent(event)}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

/** 审查卡及其操作行的 props：审查页面态 + 当前工作区（审查入口需要）。 */
type ReviewCardProps = { model: ScanReviewModel; workspace: string | null };

/** LLM 源码审查卡（参考层）：操作行 → 状态摘要 → 统计 → 筛选 / 导出 → 建议列表。 */
export function ScanReviewCard({ model, workspace }: ReviewCardProps) {
  const { review, reviewLimit, reviewComplete, suggestions, shownSuggestions, reviewError } =
    model;
  const reviewResult = review;
  return (
    <section className="card review-card">
      <header className="card-head">
        <div>
          <h3>LLM 源码审查（参考）</h3>
          <p className="card-sub">
            每次最多 {reviewLimit} 个可审查源码文件（合计 ≤128 KiB）发送到已配置 Provider，
            覆盖安全、业务逻辑与代码质量三个维度；点击后自动分页连续审查，直至覆盖全部候选。
            「独立复核」会再对每批建议做一次独立判定，耗时与用量更高。
            源码被视为不可信数据；结果仅作参考，不写入发现、基线或门禁。
          </p>
        </div>
        <span className="discipline-tag">非确定性参考 · 不参与门禁</span>
      </header>
      <ReviewActionsRow model={model} workspace={workspace} />
      {(review || (reviewResult && !reviewComplete)) && <ReviewStatus model={model} />}
      {review && <ReviewStatsGrid model={model} />}
      {suggestions.length > 0 && <ReviewFilterToolbar model={model} />}
      {reviewError && (
        <p role="alert" className="run-error review-error">
          {reviewError}
        </p>
      )}
      {review &&
        (suggestions.length === 0 ? (
          <p className="muted review-empty">未发现问题（参考）；这不代表代码安全。</p>
        ) : shownSuggestions.length === 0 ? (
          <p className="muted review-empty">
            当前筛选下无匹配建议；可「清除过滤」查看全部 {suggestions.length} 条。
          </p>
        ) : (
          <ul className="review-list" data-testid="review-suggestions">
            {shownSuggestions.map((s) => (
              <ReviewSuggestionItem key={`${s.path}:${s.line}:${s.title}`} suggestion={s} />
            ))}
          </ul>
        ))}
    </section>
  );
}

/** 操作行：开始 / 继续审查、重置进度、文件数上限、模型（按 Profile）与独立复核开关。 */
function ReviewActionsRow({ model, workspace }: ReviewCardProps) {
  const {
    review,
    reviewRunning,
    reviewProgress,
    reviewComplete,
    reviewLimit,
    setReviewLimit,
    providers,
    reviewProviderId,
    setReviewProvider,
    verifyReview,
    setVerifyReview,
    runReview,
    resetReview,
  } = model;
  const reviewResult = review;
  return (
    <div className="row review-actions">
      <button
        className="primary"
        onClick={() =>
          workspace &&
          void runReview(
            workspace,
            reviewLimit,
            reviewResult?.next_offset ?? 0,
            reviewProviderId ?? null,
            verifyReview,
          )
        }
        disabled={!workspace || reviewRunning || reviewComplete}
      >
        {reviewRunning
          ? reviewProgress
            ? `审查中 ${reviewProgress.done}/${reviewProgress.total} 批…`
            : "审查中…"
          : reviewComplete
            ? "已审查全部候选"
            : reviewResult
              ? "继续审查"
              : "开始审查"}
      </button>
      {(reviewRunning || (reviewResult && reviewResult.files_reviewed > 0)) && (
        <button className="ghost" onClick={() => resetReview()}>
          重置进度
        </button>
      )}
      <label className="inline-check muted">
        文件数
        <select
          aria-label="审查文件上限"
          value={reviewLimit}
          onChange={(e) => setReviewLimit(Number(e.target.value))}
          disabled={reviewRunning}
        >
          <option value={12}>12</option>
          <option value={24}>24</option>
        </select>
      </label>
      {providers && providers.profiles.length > 0 && (
        <label className="inline-check muted">
          模型
          <select
            aria-label="审查模型"
            data-testid="review-provider"
            value={reviewProviderId ?? providers.active ?? ""}
            onChange={(e) => setReviewProvider(e.target.value || null)}
            disabled={reviewRunning}
          >
            {providers.profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} · {p.model}
              </option>
            ))}
          </select>
        </label>
      )}
      <label
        className="inline-check muted"
        title="对每批建议再做一次独立判定；会增加请求与耗时"
      >
        <input
          type="checkbox"
          aria-label="独立复核"
          checked={verifyReview}
          onChange={(e) => setVerifyReview(e.target.checked)}
          disabled={reviewRunning}
        />
        独立复核
      </label>
    </div>
  );
}

/** 状态摘要：覆盖进度、超大文件盲区与过滤计数，以及续审预算提示。 */
function ReviewStatus({ model }: { model: ScanReviewModel }) {
  const { review, reviewRunning, reviewComplete, suggestions, verificationCounts } = model;
  const reviewResult = review;
  return (
    <div className="review-status">
      {review && (
        <p className="muted review-summary">
          已审查 {review.files_reviewed} / {review.candidate_files} 个可审查文件
          {review.skipped_large > 0 &&
            ` · 另有 ${review.skipped_large} 个超大文件（>64 KiB）未纳入审查`}
          {review.dropped > 0 && ` · 过滤 ${review.dropped} 条无效`}
          {review.verification_enabled &&
            (suggestions.length > 0 || review.rejected > 0) &&
            ` · 复核：确认 ${verificationCounts.confirmed} 条 / 待验证 ${verificationCounts.needs} 条`}
          {review.rejected > 0 && ` · 复核过滤 ${review.rejected} 条`}
        </p>
      )}
      {reviewResult && !reviewComplete && (
        <p className="muted review-budget">
          {reviewRunning
            ? `自动续审中，已覆盖 ${reviewResult.next_offset} / ${reviewResult.candidate_files}；可随时「重置进度」中止并清空。`
            : `还有 ${reviewResult.candidate_files - reviewResult.next_offset} 个候选未审查，可点「继续审查」继续覆盖；也可增大文件数上限减少次数。`}
        </p>
      )}
    </div>
  );
}

/** 审查统计网格：与扫描结果统计网格同构（建议 / 错误 / 警告 / 提示）。 */
function ReviewStatsGrid({ model }: { model: ScanReviewModel }) {
  const { suggestions, severityCounts } = model;
  return (
    <div className="stats-grid" data-testid="review-stats">
      <div className="stat stat-suggestions">
        <strong>{suggestions.length}</strong>
        <span>建议</span>
      </div>
      <div className="stat stat-error">
        <strong>{severityCounts.error}</strong>
        <span>{SEVERITY_LABEL.error}</span>
      </div>
      <div className="stat stat-warning">
        <strong>{severityCounts.warning}</strong>
        <span>{SEVERITY_LABEL.warning}</span>
      </div>
      <div className="stat stat-info">
        <strong>{severityCounts.info}</strong>
        <span>{SEVERITY_LABEL.info}</span>
      </div>
    </div>
  );
}

/** 筛选工具栏：严重级 / 维度 / 置信度 / 关键词，计数与导出。 */
function ReviewFilterToolbar({ model }: { model: ScanReviewModel }) {
  const {
    suggestions,
    shownSuggestions,
    reviewFilter,
    setReviewFilter,
    exporting,
    setExportPickerOpen,
  } = model;
  return (
    <div className="toolbar review-filter" data-testid="review-filter">
      <select
        aria-label="审查严重级过滤"
        value={reviewFilter.severity}
        onChange={(e) =>
          setReviewFilter((f) => ({
            ...f,
            severity: e.target.value as ReviewFilter["severity"],
          }))
        }
      >
        <option value="all">全部严重级</option>
        <option value="error">错误</option>
        <option value="warning">警告</option>
        <option value="info">提示</option>
      </select>
      <select
        aria-label="审查维度过滤"
        value={reviewFilter.category}
        onChange={(e) =>
          setReviewFilter((f) => ({
            ...f,
            category: e.target.value as ReviewFilter["category"],
          }))
        }
      >
        <option value="all">全部维度</option>
        <option value="security">安全</option>
        <option value="business-logic">业务逻辑</option>
        <option value="quality">质量</option>
      </select>
      <select
        aria-label="审查置信度过滤"
        value={reviewFilter.confidence}
        onChange={(e) =>
          setReviewFilter((f) => ({
            ...f,
            confidence: e.target.value as ReviewFilter["confidence"],
          }))
        }
      >
        <option value="all">全部置信度</option>
        <option value="high">高</option>
        <option value="medium">中</option>
        <option value="low">低</option>
      </select>
      <input
        aria-label="审查搜索"
        className="search-input"
        placeholder="搜索路径 / 标题 / 说明"
        value={reviewFilter.query}
        onChange={(e) => setReviewFilter((f) => ({ ...f, query: e.target.value }))}
      />
      {hasReviewFilter(reviewFilter) && (
        <button
          className="ghost"
          onClick={() => setReviewFilter(EMPTY_REVIEW_FILTER)}
        >
          清除过滤
        </button>
      )}
      <div className="toolbar-spacer" />
      <span className="toolbar-count muted" data-testid="review-filter-count">
        显示 {shownSuggestions.length} / {suggestions.length} 条
      </span>
      <button
        className="ghost"
        data-testid="review-export"
        disabled={exporting}
        onClick={() => setExportPickerOpen(true)}
      >
        {exporting ? "导出中…" : "导出"}
      </button>
    </div>
  );
}

function ReviewSuggestionItem({ suggestion: s }: { suggestion: ReviewSuggestion }) {
  return (
    <li>
      <div className="review-head">
        <span className={`sev-chip sev-${s.severity}`}>
          {SEVERITY_LABEL[s.severity]}
        </span>
        <span className="dim-badge">{DIMENSION_LABEL[s.category]}</span>
        {s.verification && (
          <span className={`verify-chip verify-${s.verification}`}>
            {VERIFICATION_LABEL[s.verification]}
          </span>
        )}
        <span className="path" title={`${s.path}:${s.line}`}>
          {s.path}:{s.line}
        </span>
        <span className="review-head-actions">
          <CopyButton
            label={`复制审查结果：${s.path}:${s.line}`}
            text={[
              `${SEVERITY_LABEL[s.severity]}｜${DIMENSION_LABEL[s.category]}｜${s.path}:${s.line}`,
              s.title,
              s.detail,
              `置信度：${CONFIDENCE_LABEL[s.confidence]}`,
              ...(s.verification
                ? [`复核：${VERIFICATION_LABEL[s.verification]}`]
                : []),
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
  );
}
