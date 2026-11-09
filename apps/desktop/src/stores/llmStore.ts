/** LLM 辅助层状态：Provider 配置、分诊结果、候选规则（均不入门禁）。 */
import { create } from "zustand";
import {
  api,
  platform,
  type BatchTriageEntry,
  type CandidateRule,
  type ProviderInfo,
  type TriageResult,
} from "../platform";

type LlmState = {
  provider: ProviderInfo | null;
  triage: Record<string, TriageResult | "loading" | null>;
  /** 流式分诊已接收的增量文本（按 fingerprint）。 */
  triageStreams: Record<string, string>;
  streamPending: Record<string, boolean>;
  candidates: Record<string, CandidateRule | "loading" | null>;
  batch: BatchTriageEntry[] | "loading" | null;
  batchError: string | null;
  errors: Record<string, string | null>;
  loadProvider: () => Promise<void>;
  saveProvider: (kind: string, baseUrl: string, model: string) => Promise<void>;
  saveApiKey: (key: string) => Promise<void>;
  clearApiKey: () => Promise<void>;
  runTriage: (scanId: number, fingerprint: string) => Promise<void>;
  runTriageStream: (scanId: number, fingerprint: string) => Promise<void>;
  runBatchTriage: (scanId: number, fingerprints: string[]) => Promise<void>;
  generateRule: (scanId: number, fingerprint: string) => Promise<void>;
  saveRule: (fingerprint: string, yaml: string) => Promise<void>;
};

export const useLlmStore = create<LlmState>((set, get) => ({
  provider: null,
  triage: {},
  triageStreams: {},
  streamPending: {},
  candidates: {},
  batch: null,
  batchError: null,
  errors: {},

  async loadProvider() {
    const provider = await api.getProviderConfig();
    set({ provider });
  },

  async saveProvider(kind, baseUrl, model) {
    await api.setProviderConfig(kind, baseUrl, model);
    await get().loadProvider();
  },

  async saveApiKey(key) {
    await api.setApiKey(key);
    await get().loadProvider();
  },

  async clearApiKey() {
    await api.clearApiKey();
    await get().loadProvider();
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

  async saveRule(fingerprint, yaml) {
    try {
      await api.saveCustomRule(yaml);
      set({ errors: { ...get().errors, [fingerprint]: null } });
    } catch (e) {
      set({ errors: { ...get().errors, [fingerprint]: String(e) } });
    }
  },
}));
