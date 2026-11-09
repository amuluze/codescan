/** LLM 辅助层状态：Provider 配置、分诊结果、候选规则（均不入门禁）。 */
import { create } from "zustand";
import { api, type CandidateRule, type ProviderInfo, type TriageResult } from "../platform";

type LlmState = {
  provider: ProviderInfo | null;
  triage: Record<string, TriageResult | "loading" | null>;
  candidates: Record<string, CandidateRule | "loading" | null>;
  errors: Record<string, string | null>;
  loadProvider: () => Promise<void>;
  saveProvider: (kind: string, baseUrl: string, model: string) => Promise<void>;
  saveApiKey: (key: string) => Promise<void>;
  clearApiKey: () => Promise<void>;
  runTriage: (scanId: number, fingerprint: string) => Promise<void>;
  generateRule: (scanId: number, fingerprint: string) => Promise<void>;
  saveRule: (fingerprint: string, yaml: string) => Promise<void>;
};

export const useLlmStore = create<LlmState>((set, get) => ({
  provider: null,
  triage: {},
  candidates: {},
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
