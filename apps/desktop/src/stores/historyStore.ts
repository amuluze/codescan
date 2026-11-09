/** 扫描历史、基线与门禁差异。 */
import { create } from "zustand";
import { api, type ScanSummary } from "../platform";
import type { Finding } from "../types";
import type { BaselineInfo } from "../platform";

type HistoryState = {
  scans: ScanSummary[];
  baseline: BaselineInfo | null;
  openScanId: number | null;
  openFindings: Finding[];
  gateNew: Finding[] | null;
  loading: boolean;
  load: () => Promise<void>;
  openScan: (scanId: number) => Promise<void>;
  setBaseline: (scanId: number) => Promise<void>;
  refreshGate: (scanId: number) => Promise<void>;
};

export const useHistoryStore = create<HistoryState>((set, get) => ({
  scans: [],
  baseline: null,
  openScanId: null,
  openFindings: [],
  gateNew: null,
  loading: false,

  async load() {
    set({ loading: true });
    try {
      const [scans, baseline] = await Promise.all([
        api.listScans(),
        api.baselineInfo().catch(() => null),
      ]);
      set({ scans, baseline, loading: false });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },

  async openScan(scanId) {
    const findings = await api.getScan(scanId);
    set({ openScanId: scanId, openFindings: findings, gateNew: null });
    await get().refreshGate(scanId);
  },

  async setBaseline(scanId) {
    await api.setBaseline(scanId);
    const baseline = await api.baselineInfo();
    set({ baseline });
  },

  async refreshGate(scanId) {
    const gateNew = await api.gateDiff(scanId);
    set({ gateNew });
  },
}));
