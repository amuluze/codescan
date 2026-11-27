/** 扫描会话状态机：idle → scanning → finished | cancelled | error。 */
import { create } from "zustand";
import {
  api,
  platform,
  type ProgressPayload,
  type ScanSummary,
} from "../platform";
import type { Finding, ScanEvent, ScanStats } from "../types";

/** 历史工作区快捷下拉的最大条目数。 */
const RECENT_WORKSPACE_LIMIT = 8;

/** 从扫描历史提取工作区：按最近扫描时间去重排序，最新在前。 */
function recentRootsFrom(scans: ScanSummary[]): string[] {
  const latest = new Map<string, number>();
  for (const scan of scans) {
    const prev = latest.get(scan.root);
    if (prev === undefined || scan.started_at_ms > prev) {
      latest.set(scan.root, scan.started_at_ms);
    }
  }
  return [...latest.entries()]
    .sort(([, a], [, b]) => b - a)
    .slice(0, RECENT_WORKSPACE_LIMIT)
    .map(([root]) => root);
}

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
  /** 历史扫描过的工作区（去重、最新在前）；扫描页快捷选择用。 */
  recentRoots: string[];
  loadRecentRoots: () => Promise<void>;
  progress: ProgressPayload | null;
  error: string | null;
  lastScanId: number | null;
  stats: ScanStats | null;
  findings: Finding[];
  /** 扫描事件（解析/IO/二进制/超限/忽略/未知语言），来自持久化历史。 */
  events: ScanEvent[];
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
  recentRoots: [],
  /** 历史工作区（去重、最新在前）；读取失败静默，不阻断扫描页。 */
  loadRecentRoots: async () => {
    try {
      const scans = await api.listScans();
      set({ recentRoots: recentRootsFrom(scans) });
    } catch {
      // 历史读取失败不阻断扫描页：快捷选择是可选便利，浏览目录不受影响。
    }
  },
  progress: null,
  error: null,
  lastScanId: null,
  stats: null,
  findings: [],
  events: [],
  filter: { dimension: "all", severity: "all", query: "" },
  selected: null,

  async initListeners() {
    await platform.listen("scan://progress", (payload) => {
      set({ progress: payload as ProgressPayload });
    });
    await platform.listen("scan://finished", async (payload) => {
      if (!isFinishedPayload(payload)) return;
      await get().loadScan(payload.scan_id, payload.stats);
      // 本次扫描已入历史：刷新快捷列表，让刚扫过的工作区排在最前。
      void get().loadRecentRoots();
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
      events: [],
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
      const [findings, events] = await Promise.all([
        api.getScan(scanId),
        api.getScanEvents(scanId).catch(() => [] as ScanEvent[]),
      ]);
      set({
        phase: "finished",
        progress: null,
        lastScanId: scanId,
        findings,
        events,
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
