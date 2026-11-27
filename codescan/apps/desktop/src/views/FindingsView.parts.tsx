/**
 * 发现页的区段组件：工具栏、批量分诊卡、发现列表与详情（含 LLM 辅助面板）。
 * 均为 props 驱动、无自有状态；页面骨架、选中项校正与导出见 FindingsView.tsx。
 */
import type { Dispatch, SetStateAction } from "react";
import { CopyButton } from "../components/CopyButton";
import { DIMENSION_LABEL, SEVERITY_LABEL } from "../labels";
import type {
  BatchTriageEntry,
  CandidateRegression,
  CandidateRule,
  TriageResult,
} from "../platform";
import type { Finding } from "../types";
import type { FindingFilter } from "../stores/scanStore";

const DIMENSIONS: Array<"all" | Finding["dimension"]> = [
  "all",
  "security",
  "business-logic",
  "quality",
];
const SEVERITIES: Array<"all" | Finding["severity"]> = ["all", "error", "warning", "info"];

/** 详情面板用到的 LLM 辅助状态与动作（llmStore 的分诊 / 流式 / 候选规则 / 回归切片）。 */
type FindingLlmAssist = {
  triage: Record<string, TriageResult | "loading" | null>;
  /** 流式分诊已接收的增量文本（按 fingerprint）。 */
  triageStreams: Record<string, string>;
  streamPending: Record<string, boolean>;
  candidates: Record<string, CandidateRule | "loading" | null>;
  regressions: Record<string, CandidateRegression | "loading" | null>;
  errors: Record<string, string | null>;
  runTriage: (scanId: number, fingerprint: string) => Promise<void>;
  runTriageStream: (scanId: number, fingerprint: string) => Promise<void>;
  generateRule: (scanId: number, fingerprint: string) => Promise<void>;
  runRegression: (
    scanId: number,
    fingerprint: string,
    candidateYaml: string,
    saveFixture: boolean,
  ) => Promise<void>;
  saveRule: (fingerprint: string, yaml: string) => Promise<void>;
};

type FindingsToolbarProps = {
  filter: FindingFilter;
  onFilterChange: (patch: Partial<FindingFilter>) => void;
  shown: Finding[];
  scanId: number | null;
  exporting: boolean;
  batch: BatchTriageEntry[] | "loading" | null;
  onExport: () => void;
  onBatchTriage: (scanId: number, fingerprints: string[]) => Promise<void>;
};

/** 工具栏：维度 / 严重级 / 搜索筛选、计数、导出与批量分诊（最多取前 20 条）。 */
export function FindingsToolbar({
  filter,
  onFilterChange,
  shown,
  scanId,
  exporting,
  batch,
  onExport,
  onBatchTriage,
}: FindingsToolbarProps) {
  const hasFilter =
    filter.dimension !== "all" || filter.severity !== "all" || filter.query.trim() !== "";
  return (
    <div className="toolbar">
      <select
        aria-label="维度过滤"
        value={filter.dimension}
        onChange={(e) =>
          onFilterChange({ dimension: e.target.value as Finding["dimension"] | "all" })
        }
      >
        {DIMENSIONS.map((d) => (
          <option key={d} value={d}>
            {d === "all" ? "全部维度" : DIMENSION_LABEL[d]}
          </option>
        ))}
      </select>
      <select
        aria-label="严重级过滤"
        value={filter.severity}
        onChange={(e) =>
          onFilterChange({ severity: e.target.value as Finding["severity"] | "all" })
        }
      >
        {SEVERITIES.map((s) => (
          <option key={s} value={s}>
            {s === "all" ? "全部严重级" : SEVERITY_LABEL[s]}
          </option>
        ))}
      </select>
      <input
        aria-label="搜索"
        className="search-input"
        placeholder="搜索路径 / 规则 / 消息"
        value={filter.query}
        onChange={(e) => onFilterChange({ query: e.target.value })}
      />
      {hasFilter && (
        <button
          className="ghost"
          onClick={() => onFilterChange({ dimension: "all", severity: "all", query: "" })}
        >
          清除过滤
        </button>
      )}
      <div className="toolbar-spacer" />
      <span className="toolbar-count muted">显示 {shown.length} 条</span>
      <button
        className="ghost"
        data-testid="findings-export"
        disabled={!scanId || shown.length === 0 || exporting}
        onClick={onExport}
      >
        {exporting ? "导出中…" : "导出"}
      </button>
      <button
        disabled={!scanId || batch === "loading" || shown.length === 0}
        onClick={() =>
          scanId &&
          void onBatchTriage(
            scanId,
            shown.slice(0, 20).map((f) => f.fingerprint),
          )
        }
      >
        批量分诊
      </button>
    </div>
  );
}

type BatchTriageProps = {
  batch: BatchTriageEntry[] | "loading" | null;
  batchError: string | null;
  shown: Finding[];
};

/** 批量分诊：错误 / 进行中提示 / 结果卡（参考层，不参与门禁）。 */
export function BatchTriageSection({ batch, batchError, shown }: BatchTriageProps) {
  return (
    <>
      {batchError && <p role="alert">{batchError}</p>}
      {batch === "loading" && <p className="muted">批量分诊中…</p>}
      {batch && batch !== "loading" && (
        <div className="card batch-triage" data-testid="batch-triage">
          <header className="card-head">
            <h3>批量分诊（参考）</h3>
            <span className="discipline-tag">非确定性参考 · 不参与门禁</span>
          </header>
          <ul>
            {batch.map((entry, index) => (
              <BatchTriageItem key={entry.fingerprint} entry={entry} index={index} shown={shown} />
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

function BatchTriageItem({
  entry,
  index,
  shown,
}: {
  entry: BatchTriageEntry;
  index: number;
  shown: Finding[];
}) {
  const finding = shown.find((f) => f.fingerprint === entry.fingerprint);
  const label = finding
    ? `${finding.path}:${finding.span.start.line}`
    : entry.fingerprint;
  const assessment = entry.missing ? "missing" : entry.assessment;
  return (
    <li className="batch-entry">
      <div>
        <span className="path">{label}</span>{" "}
        <span className="assessment">{assessment}</span>{" "}
        <span className="muted">{entry.rationale}</span>
      </div>
      <CopyButton
        text={`${label} | ${assessment} | ${entry.rationale}`}
        label={`复制批量分诊第 ${index + 1} 条`}
      />
    </li>
  );
}

export function FindingList({
  findings,
  total,
  selected,
  onSelect,
}: {
  /** 当前筛选下可见的发现。 */
  findings: Finding[];
  /** 未过滤的发现总数：决定空态文案。 */
  total: number;
  selected: Finding | null;
  onSelect: (finding: Finding) => void;
}) {
  return (
    <div className="finding-table">
      <div className="finding-table-head" aria-hidden="true">
        <span>严重级</span>
        <span>维度</span>
        <span>位置</span>
        <span>规则</span>
      </div>
      <ul className="finding-list" data-testid="finding-list">
        {findings.map((f) => (
          <li
            key={f.fingerprint}
            className={selected?.fingerprint === f.fingerprint ? "selected" : ""}
            onClick={() => onSelect(f)}
          >
            <span className={`sev-chip sev-${f.severity}`}>
              {SEVERITY_LABEL[f.severity]}
            </span>
            <span className="dim-badge">{DIMENSION_LABEL[f.dimension]}</span>
            <span className="path" title={`${f.path}:${f.span.start.line}`}>
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
              <strong>无匹配发现</strong>
              <p className="muted">
                {total === 0
                  ? "先在「扫描」页选择工作区并运行一次扫描。"
                  : "当前过滤条件下没有结果，尝试放宽维度 / 严重级或清空搜索。"}
              </p>
            </div>
          </li>
        )}
      </ul>
    </div>
  );
}

type FindingDetailProps = {
  finding: Finding;
  scanId: number | null;
  llm: FindingLlmAssist;
  saveFixture: boolean;
  setSaveFixture: Dispatch<SetStateAction<boolean>>;
};

/** 发现详情：规则 / 位置 / 标签、消息、代码证据、捕获变量、污点来源与 LLM 辅助面板。 */
export function FindingDetail(props: FindingDetailProps) {
  const { finding } = props;
  return (
    <aside className="finding-detail" data-testid="finding-detail">
      <header className="detail-head">
        <div>
          <h3>{finding.rule_id}</h3>
          <p className="muted">
            {finding.path}:{finding.span.start.line}-{finding.span.end.line}
          </p>
        </div>
        <div className="detail-tags">
          <span className={`sev-chip sev-${finding.severity}`}>
            {SEVERITY_LABEL[finding.severity]}
          </span>
          <span className="dim-badge">{DIMENSION_LABEL[finding.dimension]}</span>
        </div>
      </header>
      <p className="detail-message">{finding.message}</p>
      <pre data-testid="match-text" className="code-block">
        <code>{finding.match_text}</code>
      </pre>
      {finding.captures.length > 0 && (
        <dl className="captures">
          {finding.captures.map(([name, text]) => (
            <div key={name}>
              <dt>@{name}</dt>
              <dd>{text}</dd>
            </div>
          ))}
        </dl>
      )}
      {finding.flows.length > 0 && (
        <div className="detail-section" data-testid="taint-flows">
          <h4>污点来源（过程内近似）</h4>
          <ul>
            {finding.flows.map((p) => (
              <li key={`${p.line}:${p.column}`}>
                第 {p.line} 行第 {p.column} 列 → 当前 sink
              </li>
            ))}
          </ul>
        </div>
      )}
      <FindingLlmPanel {...props} />
    </aside>
  );
}

/** LLM 辅助面板：分诊 / 流式分诊 / 候选规则与隔离回归（参考层，不参与门禁与基线）。 */
function FindingLlmPanel({
  finding,
  scanId,
  llm,
  saveFixture,
  setSaveFixture,
}: FindingDetailProps) {
  const {
    triage,
    triageStreams,
    streamPending,
    candidates,
    regressions,
    errors,
    runTriage,
    runTriageStream,
    generateRule,
  } = llm;
  const { fingerprint } = finding;
  const triageValue = triage[fingerprint];
  const streamValue = triageStreams[fingerprint] ?? "";
  const candidate = candidates[fingerprint];
  const regression = regressions[fingerprint];
  return (
    <div className="detail-section llm-panel">
      <h4>LLM 辅助</h4>
      <div className="row">
        <button
          disabled={!scanId || triage[fingerprint] === "loading"}
          onClick={() => scanId && void runTriage(scanId, fingerprint)}
        >
          LLM 分诊
        </button>
        <button
          disabled={!scanId || streamPending[fingerprint]}
          onClick={() => scanId && void runTriageStream(scanId, fingerprint)}
        >
          流式分诊
        </button>
        <button
          disabled={!scanId || candidates[fingerprint] === "loading"}
          onClick={() => scanId && void generateRule(scanId, fingerprint)}
        >
          生成候选规则
        </button>
      </div>
      <p className="discipline-tag">非确定性参考 · 不参与门禁与基线；仅保存为确定性规则后才影响后续扫描</p>
      {errors[fingerprint] && (
        <p role="alert">{errors[fingerprint]}</p>
      )}
      {streamPending[fingerprint] && (
        <p className="muted">流式分诊接收中…</p>
      )}
      {streamValue && (
        <>
          <div className="output-tools">
            <CopyButton text={streamValue} label="复制流式分诊输出" />
          </div>
          <pre data-testid="triage-stream" className="code-block triage-stream">
            <code>{streamValue}</code>
          </pre>
        </>
      )}
      {triage[fingerprint] === "loading" && <p className="muted">分诊中…</p>}
      {triageValue && triageValue !== "loading" && (
        <div data-testid="triage-result">
          <div className="detail-section-head">
            <h4>分诊建议（参考）</h4>
            <CopyButton
              text={`分诊：${triageValue.assessment}\n理由：${triageValue.rationale}`}
              label="复制分诊建议"
            />
          </div>
          <p className="assessment">{triageValue.assessment}</p>
          <p className="muted">{triageValue.rationale}</p>
        </div>
      )}
      {candidate === "loading" && (
        <p className="muted">生成候选规则中…</p>
      )}
      {candidate && candidate !== "loading" && (
        <div data-testid="candidate-rule">
          <CandidateRuleBlock
            candidate={candidate}
            regression={regression}
            llm={llm}
            scanId={scanId}
            fingerprint={fingerprint}
            saveFixture={saveFixture}
            setSaveFixture={setSaveFixture}
          />
        </div>
      )}
    </div>
  );
}

type CandidateRuleBlockProps = {
  candidate: CandidateRule;
  regression: CandidateRegression | "loading" | null;
} & Pick<FindingDetailProps, "llm" | "scanId" | "saveFixture" | "setSaveFixture"> & {
  fingerprint: string;
};

/** 候选规则：校验结果、YAML、隔离回归（可选保存夹具）与「保存为自定义规则」。 */
function CandidateRuleBlock({
  candidate: c,
  regression,
  llm,
  scanId,
  fingerprint,
  saveFixture,
  setSaveFixture,
}: CandidateRuleBlockProps) {
  const { runRegression, saveRule } = llm;
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
      <pre className="code-block rule-yaml">
        <code>{c.yaml}</code>
      </pre>
      <div className="row">
        <button
          disabled={!c.valid || regression === "loading"}
          onClick={() =>
            scanId && void runRegression(scanId, fingerprint, c.yaml, saveFixture)
          }
        >
          运行回归验证
        </button>
        <label className="inline-check muted">
          <input
            type="checkbox"
            aria-label="保存回归夹具"
            checked={saveFixture}
            onChange={(e) => setSaveFixture(e.target.checked)}
          />
          保存夹具
        </label>
      </div>
      {regression === "loading" && (
        <p className="muted">隔离回归中…</p>
      )}
      <RegressionResult regression={regression} />
      <button
        className="primary"
        disabled={!c.valid}
        onClick={() => void saveRule(fingerprint, c.yaml)}
      >
        保存为自定义规则
      </button>
    </>
  );
}

/** 隔离回归结果（仅展示，不参与门禁 / 基线）；进行中或尚无结果时不渲染。 */
function RegressionResult({
  regression: r,
}: {
  regression: CandidateRegression | "loading" | null;
}) {
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
            期望行：{r.expected.length > 0 ? r.expected.join(", ") : "无"}
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
}
