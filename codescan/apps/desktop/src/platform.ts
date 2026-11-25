/**
 * 平台层：所有 Tauri IPC 经此单向封装，UI 测试通过 vi.mock("./platform") 替换。
 * 浏览器开发模式（无 Tauri）下调invoke 会失败——UI 需容忍并展示错误态。
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { check as updaterCheck, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
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

export type ProviderProfileInfo = {
  id: string;
  name: string;
  kind: "openai" | "anthropic";
  base_url: string;
  model: string;
  has_key: boolean;
};

/** Provider Profile 列表与活动 Profile（Domain §3a.2）；密钥只报状态。 */
export type ProvidersInfo = {
  profiles: ProviderProfileInfo[];
  active: string | null;
};

export type ProviderTestResult = {
  kind: "openai" | "anthropic";
  model: string;
  latency_ms: number;
};

export type ReviewSuggestion = {
  path: string;
  line: number;
  severity: "error" | "warning" | "info";
  category: "security" | "business-logic" | "quality";
  title: string;
  detail: string;
  confidence: "high" | "medium" | "low";
  /** 独立复核结论；null/缺省 = 未开启复核。rejected 不会出现在结果中。 */
  verification?: "confirmed" | "needs_validation" | null;
};

export type ReviewResult = {
  files_reviewed: number;
  candidate_files: number;
  /** 因超大（>64 KiB）未纳入审查的源码文件数；工作区级常量，续审不累计。 */
  skipped_large: number;
  next_offset: number;
  total_bytes: number;
  model: string;
  dropped: number;
  /** 独立复核判定不成立而过滤的建议数。 */
  rejected: number;
  /** 本次是否启用独立复核。 */
  verification_enabled: boolean;
  suggestions: ReviewSuggestion[];
};

export type TriageResult = {
  assessment: "true-positive" | "likely-false-positive" | "uncertain";
  rationale: string;
};

/** 内置审查技能文件（设置页展示与编辑）。content 为当前生效正文。 */
export type ReviewSkillFileInfo = {
  name: string;
  title: string;
  content: string;
  customized: boolean;
  bytes: number;
};

export type ReviewSkillInfo = {
  version: string;
  files: ReviewSkillFileInfo[];
  max_file_bytes: number;
  max_total_bytes: number;
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

export type UpdateInfo = { version: string; notes: string };

/** 检查到的更新对象须跨调用保留（downloadAndInstall 依赖同一实例）。 */
let pendingUpdate: Update | null = null;
/** 安装进行中：期间不释放 pendingUpdate（下载依赖其持有的 Rust 侧资源）。 */
let installing = false;

/**
 * 自更新：端点为 GitHub Releases 的 latest.json（draft 发布后生效），更新包
 * 经 minisign 签名校验（tauri.release.conf.json 的 plugins.updater.pubkey）。
 * 开发模式 / 无 updater 配置构建下 check 会失败——由 UI 容忍并展示错误态。
 */
export const updater = {
  /** 检查更新；无更新返回 null，失败向上抛错由 UI 展示。 */
  async check(): Promise<UpdateInfo | null> {
    const update = await updaterCheck();
    const previous = pendingUpdate;
    pendingUpdate = update;
    // 每次检查都会在 Rust 侧新建 update 资源；被替换的旧实例必须显式释放，
    // 否则定期检查会持续堆积（安装进行中除外：下载依赖该资源）。
    if (previous && previous !== update && !installing) {
      void previous.close().catch(() => {});
    }
    if (!update) return null;
    return { version: update.version, notes: update.body ?? "" };
  },

  /** 下载并安装已检查到的更新，随后重启应用。 */
  async installAndRelaunch(): Promise<void> {
    const update = pendingUpdate;
    if (!update) throw new Error("没有待安装的更新（请先检查更新）");
    installing = true;
    try {
      await update.downloadAndInstall();
      await relaunch();
    } finally {
      installing = false;
    }
  },
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
  deleteScan: (scanId: number) => platform.invoke<void>("delete_scan", { scanId }),
  setBaseline: (scanId: number) => platform.invoke<number>("set_baseline", { scanId }),
  gateDiff: (scanId: number) =>
    platform.invoke<Finding[] | null>("gate_diff", { scanId }),
  baselineInfo: () => platform.invoke<BaselineInfo | null>("baseline_info"),
  exportSarif: (scanId: number, dest: string) =>
    platform.invoke<void>("export_sarif", { scanId, dest }),
  /** 展示层报告导出（审查建议 / 发现视图）：Markdown+JSON 双文件写入选定目录。 */
  exportReport: (destDir: string, base: string, markdown: string, json: string) =>
    platform.invoke<void>("export_report", { destDir, base, markdown, json }),
  listDirectory: (path?: string) =>
    platform.invoke<DirListing>("list_directory", { path: path ?? null }),
  listProviders: () => platform.invoke<ProvidersInfo>("list_providers"),
  upsertProviderProfile: (
    profile: {
      id: string | null;
      name: string;
      kind: string;
      baseUrl: string;
      model: string;
    },
    options?: { apiKey?: string | null; clearKey?: boolean },
  ) =>
    platform.invoke<string>("upsert_provider_profile", {
      id: profile.id,
      name: profile.name,
      kind: profile.kind,
      baseUrl: profile.baseUrl,
      model: profile.model,
      apiKey: options?.apiKey ?? null,
      clearKey: options?.clearKey ?? false,
    }),
  deleteProviderProfile: (id: string) =>
    platform.invoke<void>("delete_provider_profile", { id }),
  setActiveProvider: (id: string) =>
    platform.invoke<void>("set_active_provider", { id }),
  testProviderConnection: (id?: string) =>
    platform.invoke<ProviderTestResult>("test_provider_connection", {
      profileId: id ?? null,
    }),
  llmReviewWorkspace: (
    root: string,
    maxFiles?: number,
    offset?: number,
    requestId?: string,
    providerId?: string | null,
    verify?: boolean,
  ) =>
    platform.invoke<ReviewResult>("llm_review_workspace", {
      root,
      maxFiles: maxFiles ?? null,
      offset: offset ?? 0,
      requestId: requestId ?? null,
      providerId: providerId ?? null,
      verify: verify ?? true,
    }),

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
  llmReviewSkillInfo: () =>
    platform.invoke<ReviewSkillInfo>("llm_review_skill_info"),
  saveLlmReviewSkill: (name: string, content: string) =>
    platform.invoke<ReviewSkillFileInfo>("save_llm_review_skill", {
      name,
      content,
    }),
  resetLlmReviewSkill: (name: string) =>
    platform.invoke<ReviewSkillFileInfo>("reset_llm_review_skill", { name }),
};

export type { Finding };
