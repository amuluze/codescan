/** 扫描历史、基线与门禁差异。 */
import { create } from "zustand";
import { api, type ScanRuleset, type ScanSummary } from "../platform";
import type { Finding, ScanEvent } from "../types";
import type { BaselineInfo } from "../platform";

/** 门禁差异展示态：结果始终绑定目标扫描，未设置基线不是错误。 */
export type GateState =
  | { status: "idle" }
  | { status: "loading"; scanId: number }
  | { status: "no-baseline"; scanId: number }
  | { status: "ready"; scanId: number; findings: Finding[] };

type HistoryState = {
  scans: ScanSummary[];
  /** `null`=后端明确无基线；`undefined`=未知或写入/删除后尚未确认。 */
  baseline: BaselineInfo | null | undefined;
  openScanId: number | null;
  openFindings: Finding[];
  openEvents: ScanEvent[];
  openRuleset: ScanRuleset | null;
  /** 次要元数据读取失败的可见降级说明，不伪装成“成功返回空”。 */
  openWarnings: string[];
  gate: GateState;
  loading: boolean;
  load: () => Promise<void>;
  openScan: (scanId: number) => Promise<void>;
  deleteScan: (scanId: number) => Promise<void>;
  setBaseline: (scanId: number) => Promise<void>;
  refreshGate: (scanId: number) => Promise<void>;
};

// 单调递增的用户动作版本：请求提交以版本为准，过期响应直接丢弃。
let openVersion = 0;
let gateVersion = 0;

export const useHistoryStore = create<HistoryState>((set, get) => ({
  scans: [],
  baseline: undefined,
  openScanId: null,
  openFindings: [],
  openEvents: [],
  openRuleset: null,
  openWarnings: [],
  gate: { status: "idle" },
  loading: false,

  async load() {
    set({ loading: true });
    const [scansResult, baselineResult] = await Promise.allSettled([
      api.listScans(),
      api.baselineInfo(),
    ]);
    if (scansResult.status === "rejected") {
      set({ loading: false });
      throw scansResult.reason;
    }

    const patch: Partial<HistoryState> = {
      scans: scansResult.value,
      loading: false,
    };
    if (baselineResult.status === "fulfilled") {
      patch.baseline = baselineResult.value;
    } else {
      // 读取失败不等于“未设置基线”：页面用 undefined 展示未知态。
      patch.baseline = undefined;
    }
    set(patch);
    if (baselineResult.status === "rejected") throw baselineResult.reason;
  },

  async openScan(scanId) {
    const requestVersion = ++openVersion;
    const gateVersionAtOpen = ++gateVersion;
    set({
      openScanId: null,
      openFindings: [],
      openEvents: [],
      openRuleset: null,
      openWarnings: [],
      gate: { status: "idle" },
    });

    const [findingsResult, eventsResult, rulesetResult] = await Promise.allSettled([
      api.getScan(scanId),
      api.getScanEvents(scanId),
      api.getScanRuleset(scanId),
    ]);
    if (requestVersion !== openVersion) return;
    if (findingsResult.status === "rejected") throw findingsResult.reason;

    const warnings: string[] = [];
    if (eventsResult.status === "rejected") {
      warnings.push(`扫描事件不可用：${String(eventsResult.reason)}`);
    }
    if (rulesetResult.status === "rejected") {
      warnings.push(`规则集不可用：${String(rulesetResult.reason)}`);
    }
    set({
      openScanId: scanId,
      openFindings: findingsResult.value,
      openEvents: eventsResult.status === "fulfilled" ? eventsResult.value : [],
      openRuleset: rulesetResult.status === "fulfilled" ? rulesetResult.value : null,
      openWarnings: warnings,
    });

    // 自动差异继承本次查看的意图；期间更晚的查看/显式差异/基线变更都会改变版本。
    if (gateVersion !== gateVersionAtOpen) return;
    await get().refreshGate(scanId);
  },

  async deleteScan(scanId) {
    await api.deleteScan(scanId);
    ++gateVersion;
    set((state) => ({
      baseline: undefined,
      gate: { status: "idle" },
      ...(state.openScanId === scanId
        ? {
            openScanId: null,
            openFindings: [],
            openEvents: [],
            openRuleset: null,
            openWarnings: [],
          }
        : {}),
    }));

    const [scansResult, baselineResult] = await Promise.allSettled([
      api.listScans(),
      api.baselineInfo(),
    ]);
    const patch: Partial<HistoryState> = {};
    if (scansResult.status === "fulfilled") patch.scans = scansResult.value;
    if (baselineResult.status === "fulfilled") patch.baseline = baselineResult.value;
    set(patch);
    if (scansResult.status === "rejected") throw scansResult.reason;
    if (baselineResult.status === "rejected") {
      throw new Error(`历史已删除，但基线状态刷新失败：${String(baselineResult.reason)}`);
    }
  },

  async setBaseline(scanId) {
    await api.setBaseline(scanId);
    ++gateVersion;
    set({ baseline: undefined, gate: { status: "idle" } });
    try {
      const baseline = await api.baselineInfo();
      set({ baseline });
    } catch (e) {
      // 写入已成功：不得保留按旧基线计算的差异，也不得误报“写入失败”。
      set({ baseline: undefined });
      throw new Error(`基线已切换，但信息刷新失败：${String(e)}`);
    }
  },

  async refreshGate(scanId) {
    const requestVersion = ++gateVersion;
    set({ gate: { status: "loading", scanId } });
    try {
      const findings = await api.gateDiff(scanId);
      if (requestVersion !== gateVersion) return;
      set({
        gate:
          findings === null
            ? { status: "no-baseline", scanId }
            : { status: "ready", scanId, findings },
      });
    } catch (e) {
      if (requestVersion === gateVersion) set({ gate: { status: "idle" } });
      throw e;
    }
  },
}));
