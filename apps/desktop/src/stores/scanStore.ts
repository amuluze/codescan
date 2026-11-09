/** 扫描会话状态机：idle → scanning → finished | cancelled | error。 */
import { create } from "zustand";
import { api, platform, type ProgressPayload } from "../platform";
import type { Finding, ScanStats } from "../types";

export type ScanPhase = "idle" | "scanning" | "finished" | "cancelled" | "error";

export type FindingFilter = {
  dimension: "all" | Finding["dimension"];
  severity: "all" | Finding["severity"];
  query: string;
};

function emptyStats(): ScanStats {
  return {
    files_total: 0,
    files_scanned: 0,
    files_ignored: 0,
    files_unknown_language: 0,
    findings: 0,
  };
}

/** finished 事件载荷（Rust ScanFinished，Tauri 自动转 camelCase 键）。 */
type FinishedPayload = { scan_id: number; stats: Partial<ScanStats> };

function isFinishedPayload(payload: unknown): payload is FinishedPayload {
  return (
    typeof payload === "object" &&
    payload !== null &&
    "scan_id" in payload &&
    typeof payload.scan_id === "number"
  );
}

type ScanState = {
  phase: ScanPhase;
  workspace: string | null;
  progress: ProgressPayload | null;
  error: string | null;
  lastScanId: number | null;
  stats: ScanStats | null;
  findings: Finding[];
  filter: FindingFilter;
  selected: Finding | null;
  initListeners: () => Promise<void>;
  setWorkspace: (path: string) => void;
  startScan: () => Promise<void>;
  cancelScan: () => Promise<void>;
  setFilter: (patch: Partial<FindingFilter>) => void;
  select: (finding: Finding | null) => void;
  loadScan: (scanId: number, stats?: Partial<ScanStats>) => Promise<void>;
};

export const useScanStore = create<ScanState>((set, get) => ({
  phase: "idle",
  workspace: null,
  progress: null,
  error: null,
  lastScanId: null,
  stats: null,
  findings: [],
  filter: { dimension: "all", severity: "all", query: "" },
  selected: null,

  async initListeners() {
    await platform.listen("scan://progress", (payload) => {
      set({ progress: payload as ProgressPayload });
    });
    await platform.listen("scan://finished", async (payload) => {
      if (!isFinishedPayload(payload)) return;
      await get().loadScan(payload.scan_id, payload.stats);
    });
    await platform.listen("scan://cancelled", () => {
      set({ phase: "cancelled", progress: null });
    });
    await platform.listen("scan://error", (payload) => {
      set({ phase: "error", error: String(payload), progress: null });
    });
  },

  setWorkspace(path) {
    set({ workspace: path });
  },

  async startScan() {
    const workspace = get().workspace;
    if (!workspace) return;
    set({
      phase: "scanning",
      progress: null,
      error: null,
      findings: [],
      selected: null,
      stats: null,
    });
    try {
      await api.startScan(workspace);
      // 完成由 scan://finished 事件驱动。
    } catch (e) {
      set({ phase: "error", error: String(e), progress: null });
    }
  },

  async cancelScan() {
    try {
      await api.cancelScan();
    } catch (e) {
      set({ phase: "error", error: String(e) });
    }
  },

  setFilter(patch) {
    set({ filter: { ...get().filter, ...patch } });
  },

  select(finding) {
    set({ selected: finding });
  },

  async loadScan(scanId, stats) {
    try {
      const findings = await api.getScan(scanId);
      set({
        phase: "finished",
        progress: null,
        lastScanId: scanId,
        findings,
        stats: { ...emptyStats(), ...stats, findings: findings.length },
      });
    } catch (e) {
      set({ phase: "error", error: String(e) });
    }
  },
}));

export function filteredFindings(state: {
  findings: Finding[];
  filter: FindingFilter;
}): Finding[] {
  const { findings, filter } = state;
  const query = filter.query.trim().toLowerCase();
  return findings.filter((f) => {
    if (filter.dimension !== "all" && f.dimension !== filter.dimension) return false;
    if (filter.severity !== "all" && f.severity !== filter.severity) return false;
    if (
      query &&
      !f.path.toLowerCase().includes(query) &&
      !f.rule_id.toLowerCase().includes(query) &&
      !f.message.toLowerCase().includes(query)
    ) {
      return false;
    }
    return true;
  });
}
