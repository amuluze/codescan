/** 规则目录与启停状态。 */
import { create } from "zustand";
import { api, type RuleInfo } from "../platform";

type RulesState = {
  rules: RuleInfo[];
  customYaml: string | null;
  customError: string | null;
  loading: boolean;
  load: () => Promise<void>;
  toggle: (ruleId: string, enabled: boolean) => Promise<void>;
};

export const useRulesStore = create<RulesState>((set, get) => ({
  rules: [],
  customYaml: null,
  customError: null,
  loading: false,

  async load() {
    set({ loading: true });
    try {
      const [rules, custom] = await Promise.all([
        api.listRules(),
        api.customRulesStatus().catch((err) => {
          set({ customError: String(err) });
          return null;
        }),
      ]);
      set({ rules, customYaml: custom, loading: false });
    } catch (e) {
      set({ loading: false, customError: String(e) });
    }
  },

  async toggle(ruleId, enabled) {
    await api.setRuleEnabled(ruleId, enabled);
    set({
      rules: get().rules.map((r) => (r.id === ruleId ? { ...r, enabled } : r)),
    });
  },
}));
