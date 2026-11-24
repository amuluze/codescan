/** LLM 辅助层状态：Provider Profile、分诊结果、候选规则（均不入门禁）。 */
import { create } from "zustand";
import {
  api,
  platform,
  type BatchTriageEntry,
  type CandidateRegression,
  type CandidateRule,
  type ProviderTestResult,
  type ProvidersInfo,
  type ReviewResult,
  type TriageResult,
} from "../platform";

/** 源码审查请求序号：切换工作区或重置进度后使在途请求与续审循环失效。 */
let reviewSeq = 0;

/** 跨页合并建议并按 (path,line,title) 去重。 */
function mergeSuggestions(
  a: ReviewResult["suggestions"],
  b: ReviewResult["suggestions"],
): ReviewResult["suggestions"] {
  const seen = new Set<string>();
  return [...a, ...b].filter((s) => {
    const key = `${s.path}\n${s.line}\n${s.title}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

type LlmState = {
  /** Provider Profile 列表与活动 Profile（Domain §3a.2）。 */
  providers: ProvidersInfo | null;
  testing: boolean;
  testResult: ProviderTestResult | null;
  testError: string | null;
  /** LLM 源码审查：仅展示层，不进入发现/基线。 */
  review: ReviewResult | null;
  /** 审查调用显式选择的 Profile；null = 活动 Profile。 */
  reviewProviderId: string | null;
  /** 自动分页续审进行中；「重置进度」可中止并清空。 */
  reviewRunning: boolean;
  reviewError: string | null;
  /** 分批审查进度（批次维度）。 */
  reviewProgress: { done: number; total: number } | null;
  triage: Record<string, TriageResult | "loading" | null>;
  /** 流式分诊已接收的增量文本（按 fingerprint）。 */
  triageStreams: Record<string, string>;
  streamPending: Record<string, boolean>;
  candidates: Record<string, CandidateRule | "loading" | null>;
  /** 候选规则隔离回归结果（按 fingerprint）。 */
  regressions: Record<string, CandidateRegression | "loading" | null>;
  batch: BatchTriageEntry[] | "loading" | null;
  batchError: string | null;
  errors: Record<string, string | null>;
  loadProviders: () => Promise<void>;
  /** 新建（id 为 null）或更新 Profile；返回落库 id。 */
  saveProfile: (profile: {
    id: string | null;
    name: string;
    kind: string;
    baseUrl: string;
    model: string;
  }) => Promise<string>;
  deleteProfile: (id: string) => Promise<void>;
  setActiveProfile: (id: string) => Promise<void>;
  setProfileKey: (id: string, key: string) => Promise<void>;
  clearProfileKey: (id: string) => Promise<void>;
  /** 测试指定（或活动）Profile 连通性。 */
  testConnection: (id?: string) => Promise<void>;
  setReviewProvider: (id: string | null) => void;
  runReview: (
    root: string,
    maxFiles?: number,
    offset?: number,
    providerId?: string | null,
  ) => Promise<void>;
  resetReview: () => void;
  runTriage: (scanId: number, fingerprint: string) => Promise<void>;
  runTriageStream: (scanId: number, fingerprint: string) => Promise<void>;
  runBatchTriage: (scanId: number, fingerprints: string[]) => Promise<void>;
  generateRule: (scanId: number, fingerprint: string) => Promise<void>;
  runRegression: (
    scanId: number,
    fingerprint: string,
    candidateYaml: string,
    saveFixture: boolean,
  ) => Promise<void>;
  saveRule: (fingerprint: string, yaml: string) => Promise<void>;
};

export const useLlmStore = create<LlmState>((set, get) => ({
  providers: null,
  testing: false,
  testResult: null,
  testError: null,
  review: null,
  reviewProviderId: null,
  reviewRunning: false,
  reviewError: null,
  reviewProgress: null,
  triage: {},
  triageStreams: {},
  streamPending: {},
  candidates: {},
  regressions: {},
  batch: null,
  batchError: null,
  errors: {},

  async loadProviders() {
    const providers = await api.listProviders();
    set({ providers });
  },

  async saveProfile(profile) {
    const id = await api.upsertProviderProfile(profile);
    await get().loadProviders();
    set({ testResult: null, testError: null });
    return id;
  },

  async deleteProfile(id) {
    await api.deleteProviderProfile(id);
    // 删除当前审查选择时回落活动 Profile。
    if (get().reviewProviderId === id) set({ reviewProviderId: null });
    await get().loadProviders();
  },

  async setActiveProfile(id) {
    await api.setActiveProvider(id);
    await get().loadProviders();
  },

  async setProfileKey(id, key) {
    await api.setProviderApiKey(id, key);
    await get().loadProviders();
    set({ testResult: null, testError: null });
  },

  async clearProfileKey(id) {
    await api.clearProviderApiKey(id);
    await get().loadProviders();
    set({ testResult: null, testError: null });
  },

  async testConnection(id) {
    set({ testing: true, testResult: null, testError: null });
    try {
      const result = await api.testProviderConnection(id);
      set({ testResult: result });
    } catch (e) {
      set({ testError: String(e) });
    } finally {
      set({ testing: false });
    }
  },

  setReviewProvider(id) {
    set({ reviewProviderId: id });
  },


  async runReview(root, maxFiles, offset = 0, providerId: string | null = null) {
    const seq = ++reviewSeq;
    const current = get().review;
    let accumulated: ReviewResult | null = offset > 0 && current ? current : null;
    let cursor = offset;
    set({ reviewRunning: true, reviewError: null, reviewProgress: null });
    try {
      while (true) {
        if (seq !== reviewSeq) return; // 已重置或切换工作区
        const requestId = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
        const unlisten = await platform.listen("llm://review_progress", (payload) => {
          if (
            typeof payload !== "object" ||
            payload === null ||
            !("requestId" in payload) ||
            payload.requestId !== requestId ||
            !("done" in payload) ||
            !("total" in payload) ||
            typeof payload.done !== "number" ||
            typeof payload.total !== "number"
          ) {
            return;
          }
          if (seq !== reviewSeq) return;
          set({ reviewProgress: { done: payload.done, total: payload.total } });
        });
        let result: ReviewResult;
        try {
          result = await api.llmReviewWorkspace(root, maxFiles, cursor, requestId, providerId);
        } finally {
          unlisten();
        }
        if (seq !== reviewSeq) return; // 工作区已切换，丢弃过期结果
        accumulated = accumulated
          ? {
              ...result,
              // skipped_large 是工作区级常量（每页重复计算），取最新页值，不累计。
              files_reviewed: accumulated.files_reviewed + result.files_reviewed,
              total_bytes: accumulated.total_bytes + result.total_bytes,
              dropped: accumulated.dropped + result.dropped,
              suggestions: mergeSuggestions(accumulated.suggestions, result.suggestions),
            }
          : result;
        set({ review: accumulated, reviewProgress: null });
        // 覆盖全部候选时停止；游标未推进属后端异常，防御性停止避免死循环。
        if (result.next_offset >= result.candidate_files || result.next_offset <= cursor) {
          return;
        }
        cursor = result.next_offset;
      }
    } catch (e) {
      if (seq !== reviewSeq) return;
      // 保留已累计结果，供「继续审查」从游标恢复。
      set({ reviewError: String(e) });
    } finally {
      if (seq === reviewSeq) {
        set({ reviewRunning: false, reviewProgress: null });
      }
    }
  },

  resetReview() {
    reviewSeq += 1;
    set({ review: null, reviewRunning: false, reviewError: null, reviewProgress: null });
  },

  async runTriage(scanId, fingerprint) {
    set({ triage: { ...get().triage, [fingerprint]: "loading" }, errors: { ...get().errors, [fingerprint]: null } });
    try {
      const result = await api.llmTriage(scanId, fingerprint);
      set({ triage: { ...get().triage, [fingerprint]: result } });
    } catch (e) {
      set({
        triage: { ...get().triage, [fingerprint]: null },
        errors: { ...get().errors, [fingerprint]: String(e) },
      });
    }
  },

  async runTriageStream(scanId, fingerprint) {
    const requestId = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    set({
      triageStreams: { ...get().triageStreams, [fingerprint]: "" },
      streamPending: { ...get().streamPending, [fingerprint]: true },
      errors: { ...get().errors, [fingerprint]: null },
    });
    let unlisten: (() => void) | null = null;
    try {
      unlisten = await platform.listen("llm://delta", (payload) => {
        if (
          typeof payload !== "object" ||
          payload === null ||
          !("requestId" in payload) ||
          payload.requestId !== requestId ||
          !("text" in payload) ||
          typeof payload.text !== "string"
        ) {
          return;
        }
        const current = get().triageStreams[fingerprint] ?? "";
        set({
          triageStreams: {
            ...get().triageStreams,
            [fingerprint]: current + payload.text,
          },
        });
      });
      const text = await api.llmTriageStream(scanId, fingerprint, requestId);
      const current = get().triageStreams[fingerprint] ?? "";
      set({
        triageStreams: {
          ...get().triageStreams,
          [fingerprint]: text.length > 0 ? text : current,
        },
      });
    } catch (e) {
      set({ errors: { ...get().errors, [fingerprint]: String(e) } });
    } finally {
      unlisten?.();
      set({ streamPending: { ...get().streamPending, [fingerprint]: false } });
    }
  },

  async runBatchTriage(scanId, fingerprints) {
    set({ batch: "loading", batchError: null });
    try {
      const entries = await api.llmBatchTriage(scanId, fingerprints);
      set({ batch: entries });
    } catch (e) {
      set({ batch: null, batchError: String(e) });
    }
  },

  async generateRule(scanId, fingerprint) {
    set({
      candidates: { ...get().candidates, [fingerprint]: "loading" },
      errors: { ...get().errors, [fingerprint]: null },
    });
    try {
      const result = await api.llmGenerateRule(scanId, fingerprint);
      set({ candidates: { ...get().candidates, [fingerprint]: result } });
    } catch (e) {
      set({
        candidates: { ...get().candidates, [fingerprint]: null },
        errors: { ...get().errors, [fingerprint]: String(e) },
      });
    }
  },

  async runRegression(scanId, fingerprint, candidateYaml, saveFixture) {
    set({
      regressions: { ...get().regressions, [fingerprint]: "loading" },
      errors: { ...get().errors, [fingerprint]: null },
    });
    try {
      const result = await api.llmCandidateRegression(
        scanId,
        fingerprint,
        candidateYaml,
        saveFixture,
      );
      set({ regressions: { ...get().regressions, [fingerprint]: result } });
    } catch (e) {
      set({
        regressions: { ...get().regressions, [fingerprint]: null },
        errors: { ...get().errors, [fingerprint]: String(e) },
      });
    }
  },

  async saveRule(fingerprint, yaml) {
    try {
      await api.saveCustomRule(yaml);
      set({ errors: { ...get().errors, [fingerprint]: null } });
    } catch (e) {
      set({ errors: { ...get().errors, [fingerprint]: String(e) } });
    }
  },
}));
