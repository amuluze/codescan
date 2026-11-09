/**
 * 平台层：所有 Tauri IPC 经此单向封装，UI 测试通过 vi.mock("./platform") 替换。
 * 浏览器开发模式（无 Tauri）下调invoke 会失败——UI 需容忍并展示错误态。
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type { Finding } from "./types";

export type ProgressPayload = {
  stage: "walking" | "scanning" | "finalizing";
  done?: number;
  total?: number;
};

export type ScanFinishedPayload = { scanId: number };

export type RuleInfo = {
  id: string;
  language: string;
  dimension: string;
  severity: string;
  message: string;
  enabled: boolean;
  source: string;
};

export type ScanSummary = {
  id: number;
  root: string;
  engine_version: string;
  started_at_ms: number;
  duration_ms: number;
  findings: number;
  files_scanned: number;
};

export type BaselineInfo = { engine_version: string; entries: number };

export type DirEntry = { name: string; path: string };
export type DirListing = {
  path: string;
  parent: string | null;
  entries: DirEntry[];
};

export type ProviderInfo = {
  kind: "openai" | "anthropic";
  base_url: string;
  model: string;
  has_key: boolean;
};

export type ProviderTestResult = {
  kind: "openai" | "anthropic";
  model: string;
  latency_ms: number;
};

export type TriageResult = {
  assessment: "true-positive" | "likely-false-positive" | "uncertain";
  rationale: string;
};

export type BatchTriageEntry = {
  fingerprint: string;
  assessment: TriageResult["assessment"];
  rationale: string;
  /** 模型未返回的请求指纹，或模型返回了未请求的指纹。 */
  missing: boolean;
};

export type CandidateRule = {
  yaml: string;
  valid: boolean;
  errors: string[];
};

export type RegressionActual = {
  line: number;
  message: string;
};

/** v3.3 候选规则隔离回归结果：仅展示，不参与门禁/基线。 */
export type CandidateRegression = {
  valid: boolean;
  pass: boolean;
  errors: string[];
  expected: number[];
  actual: RegressionActual[];
  failures: string[];
  saved_paths: string[];
};

export const platform = {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return tauriInvoke<T>(cmd, args);
  },

  /** 返回取消监听的函数。 */
  async listen(event: string, handler: (payload: unknown) => void): Promise<() => void> {
    const unlisten = await tauriListen(event, (e) => handler(e.payload));
    return unlisten;
  },

};

export const api = {
  listRules: () => platform.invoke<RuleInfo[]>("list_rules"),
  setRuleEnabled: (ruleId: string, enabled: boolean) =>
    platform.invoke<void>("set_rule_enabled", { ruleId, enabled }),
  customRulesStatus: () => platform.invoke<string | null>("custom_rules_status"),
  startScan: (root: string) => platform.invoke<void>("start_scan", { root }),
  cancelScan: () => platform.invoke<void>("cancel_scan"),
  listScans: () => platform.invoke<ScanSummary[]>("list_scans"),
  getScan: (scanId: number) => platform.invoke<Finding[]>("get_scan", { scanId }),
  setBaseline: (scanId: number) => platform.invoke<number>("set_baseline", { scanId }),
  gateDiff: (scanId: number) =>
    platform.invoke<Finding[] | null>("gate_diff", { scanId }),
  baselineInfo: () => platform.invoke<BaselineInfo | null>("baseline_info"),
  exportSarif: (scanId: number, dest: string) =>
    platform.invoke<void>("export_sarif", { scanId, dest }),
  listDirectory: (path?: string) =>
    platform.invoke<DirListing>("list_directory", { path: path ?? null }),
  getProviderConfig: () => platform.invoke<ProviderInfo>("get_provider_config"),
  setProviderConfig: (kind: string, baseUrl: string, model: string) =>
    platform.invoke<void>("set_provider_config", { kind, baseUrl, model }),
  setApiKey: (key: string) => platform.invoke<void>("set_api_key", { key }),
  clearApiKey: () => platform.invoke<void>("clear_api_key"),
  testProviderConnection: () =>
    platform.invoke<ProviderTestResult>("test_provider_connection"),
  llmTriage: (scanId: number, fingerprint: string) =>
    platform.invoke<TriageResult>("llm_triage", { scanId, fingerprint }),
  llmTriageStream: (scanId: number, fingerprint: string, requestId: string) =>
    platform.invoke<string>("llm_triage_stream", { scanId, fingerprint, requestId }),
  llmBatchTriage: (scanId: number, fingerprints: string[]) =>
    platform.invoke<BatchTriageEntry[]>("llm_batch_triage", { scanId, fingerprints }),
  llmGenerateRule: (scanId: number, fingerprint: string) =>
    platform.invoke<CandidateRule>("llm_generate_rule", { scanId, fingerprint }),
  llmCandidateRegression: (
    scanId: number,
    fingerprint: string,
    candidateYaml: string,
    saveFixture: boolean,
  ) =>
    platform.invoke<CandidateRegression>("llm_candidate_regression", {
      scanId,
      fingerprint,
      candidateYaml,
      saveFixture,
    }),
  saveCustomRule: (yaml: string) =>
    platform.invoke<void>("save_custom_rule", { yaml }),
};

export type { Finding };
