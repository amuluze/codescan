/**
 * 平台层：所有 Tauri IPC 经此单向封装，UI 测试通过 vi.mock("./platform") 替换。
 * 浏览器开发模式（无 Tauri）下调invoke 会失败——UI 需容忍并展示错误态。
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
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
  engineVersion: string;
  startedAtMs: number;
  durationMs: number;
  findings: number;
  filesScanned: number;
};

export type BaselineInfo = { engine_version: string; entries: number };

export const platform = {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return tauriInvoke<T>(cmd, args);
  },

  /** 返回取消监听的函数。 */
  async listen(event: string, handler: (payload: unknown) => void): Promise<() => void> {
    const unlisten = await tauriListen(event, (e) => handler(e.payload));
    return unlisten;
  },

  async pickFolder(): Promise<string | null> {
    const selected = await tauriOpen({ directory: true, multiple: false });
    return typeof selected === "string" ? selected : null;
  },

  async saveFile(defaultName: string): Promise<string | null> {
    const selected = await tauriOpen({
      directory: false,
      multiple: false,
      title: defaultName,
    });
    // v1：保存对话框简化为目录内文件名输入由导出命令处理；此处返回 null 表示取消。
    return typeof selected === "string" ? selected : null;
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
};

export type { Finding };
