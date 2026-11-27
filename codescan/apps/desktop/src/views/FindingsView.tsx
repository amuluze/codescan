/** 发现页：摘要 + 过滤 + 表格化列表 + 详情（证据 / 污点链 / LLM 辅助）。 */
import { useEffect, useMemo, useState } from "react";
import { filteredFindings, useScanStore } from "../stores/scanStore";
import { useLlmStore } from "../stores/llmStore";
import { DirectoryPicker } from "../components/DirectoryPicker";
import { NoticeDialog } from "../components/NoticeDialog";
import { findingsToJson, findingsToMarkdown } from "../findingsExport";
import { useReportExport } from "../useReportExport";
import {
  BatchTriageSection,
  FindingDetail,
  FindingList,
  FindingsToolbar,
} from "./FindingsView.parts";

export function FindingsView() {
  const state = useScanStore();
  const { filter, selected, select, setFilter } = state;
  const shown = useMemo(() => filteredFindings(state), [state]);
  const llm = useLlmStore();
  const scanId = state.lastScanId;
  const [saveFixture, setSaveFixture] = useState(false);
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  const { exporting, notice, run: runExport, dismiss } = useReportExport();

  /** 导出当前筛选/排序下的发现（Markdown + JSON），目录由 DirectoryPicker 选定。 */
  const exportFindingsTo = (dest: string) => {
    setExportPickerOpen(false);
    if (!scanId) return;
    void runExport(
      dest,
      `codescan-findings-${scanId}`,
      findingsToMarkdown(shown),
      findingsToJson(shown),
    );
  };

  const counts = useMemo(() => {
    const base = { error: 0, warning: 0, info: 0 };
    for (const f of state.findings) {
      base[f.severity] += 1;
    }
    return base;
  }, [state.findings]);

  // 过滤后保持选中项有效：选中项被过滤掉时自动落到第一条。
  useEffect(() => {
    if (shown.length === 0) {
      if (selected) select(null);
      return;
    }
    if (!selected || !shown.some((f) => f.fingerprint === selected.fingerprint)) {
      select(shown[0]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, selected?.fingerprint]);

  return (
    <section className="page findings-view" aria-label="发现">
      <header className="page-head">
        <div>
          <h2>发现</h2>
          <p className="page-sub">
            结果由确定性规则与指纹排序；点击条目查看证据、污点链与 LLM 辅助。
          </p>
        </div>
        <div className="summary-bar" data-testid="finding-summary">
          <span className="count-chip">共 {state.findings.length}</span>
          <span className="count-chip sev-error">错误 {counts.error}</span>
          <span className="count-chip sev-warning">警告 {counts.warning}</span>
          <span className="count-chip sev-info">提示 {counts.info}</span>
        </div>
      </header>

      <FindingsToolbar
        filter={filter}
        onFilterChange={setFilter}
        shown={shown}
        scanId={scanId}
        exporting={exporting}
        batch={llm.batch}
        onExport={() => setExportPickerOpen(true)}
        onBatchTriage={llm.runBatchTriage}
      />

      {exportPickerOpen && (
        <DirectoryPicker
          title="选择发现导出目录"
          confirmLabel="导出到此目录"
          onSelect={(dest) => exportFindingsTo(dest)}
          onClose={() => setExportPickerOpen(false)}
        />
      )}
      {notice && (
        <NoticeDialog
          tone={notice.tone}
          title={notice.title}
          message={notice.message}
          onClose={dismiss}
        />
      )}

      <BatchTriageSection batch={llm.batch} batchError={llm.batchError} shown={shown} />

      <div className="findings-body">
        <FindingList
          findings={shown}
          total={state.findings.length}
          selected={selected}
          onSelect={select}
        />
        {selected ? (
          <FindingDetail
            finding={selected}
            scanId={scanId}
            llm={llm}
            saveFixture={saveFixture}
            setSaveFixture={setSaveFixture}
          />
        ) : (
          <aside className="finding-detail detail-empty">
            <p className="muted">选择左侧任意发现查看详情。</p>
          </aside>
        )}
      </div>
    </section>
  );
}
