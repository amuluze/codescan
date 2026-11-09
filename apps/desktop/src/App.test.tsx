/** A8 主链路：tab 导航、规则列表/启停、扫描触发 → 进度事件 → 发现过滤。 */
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { useLlmStore } from "./stores/llmStore";
import type { Finding } from "./types";

type InvokeArgs = Record<string, unknown> | undefined;

const listeners = new Map<string, (payload: unknown) => void>();
const invokeMock = vi.fn();

vi.mock("./platform", () => {
  const platform = {
    invoke: (...args: [string, InvokeArgs?]) => invokeMock(...args),
    listen: vi.fn(async (event: string, handler: (p: unknown) => void) => {
      listeners.set(event, handler);
      return () => listeners.delete(event);
    }),
    pickFolder: vi.fn(async () => "/tmp/ws"),
    saveFile: vi.fn(async () => null),
  };
  const api = {
    listRules: () => platform.invoke("list_rules"),
    setRuleEnabled: (ruleId: string, enabled: boolean) =>
      platform.invoke("set_rule_enabled", { ruleId, enabled }),
    customRulesStatus: () => platform.invoke("custom_rules_status"),
    startScan: (root: string) => platform.invoke("start_scan", { root }),
    cancelScan: () => platform.invoke("cancel_scan"),
    listScans: () => platform.invoke("list_scans"),
    getScan: (scanId: number) => platform.invoke("get_scan", { scanId }),
    setBaseline: (scanId: number) => platform.invoke("set_baseline", { scanId }),
    gateDiff: (scanId: number) => platform.invoke("gate_diff", { scanId }),
    baselineInfo: () => platform.invoke("baseline_info"),
    exportSarif: (scanId: number, dest: string) =>
      platform.invoke("export_sarif", { scanId, dest }),
    getProviderConfig: () => platform.invoke("get_provider_config"),
    setProviderConfig: (kind: string, baseUrl: string, model: string) =>
      platform.invoke("set_provider_config", { kind, baseUrl, model }),
    setApiKey: (key: string) => platform.invoke("set_api_key", { key }),
    clearApiKey: () => platform.invoke("clear_api_key"),
    llmTriage: (scanId: number, fingerprint: string) =>
      platform.invoke("llm_triage", { scanId, fingerprint }),
    llmTriageStream: (scanId: number, fingerprint: string, requestId: string) =>
      platform.invoke("llm_triage_stream", { scanId, fingerprint, requestId }),
    llmBatchTriage: (scanId: number, fingerprints: string[]) =>
      platform.invoke("llm_batch_triage", { scanId, fingerprints }),
    llmGenerateRule: (scanId: number, fingerprint: string) =>
      platform.invoke("llm_generate_rule", { scanId, fingerprint }),
    saveCustomRule: (yaml: string) => platform.invoke("save_custom_rule", { yaml }),
  };
  return { platform, api };
});

const finding = (overrides: Partial<Finding>) => ({
  rule_id: "ts/no-eval",
  dimension: "security" as const,
  severity: "error" as const,
  path: "src/a.ts",
  span: { start: { line: 3, column: 1 }, end: { line: 3, column: 9 } },
  message: "m",
  match_text: "eval(x)",
  captures: [],
  degraded: false,
  flows: [],
  fingerprint: "fp",
  ...overrides,
});

const findings = [
  finding({ fingerprint: "fp-1", flows: [{ line: 2, column: 5 }] }),
  finding({
    rule_id: "metric/long",
    dimension: "quality",
    severity: "info",
    path: "src/b.py",
    fingerprint: "fp-2",
  }),
];

beforeEach(() => {
  listeners.clear();
  invokeMock.mockReset();
  useLlmStore.setState({
    provider: null,
    triage: {},
    triageStreams: {},
    streamPending: {},
    candidates: {},
    batch: null,
    batchError: null,
    errors: {},
  });
  invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
    switch (cmd) {
      case "list_rules":
        return [
          {
            id: "ts/no-eval",
            language: "typescript",
            dimension: "security",
            severity: "error",
            message: "调用 eval()",
            enabled: true,
            source: "(call_expression) @match",
          },
        ];
      case "custom_rules_status":
        return null;
      case "list_scans":
        return [
          {
            id: 7,
            root: "/tmp/ws",
            engine_version: "0.1.0",
            started_at_ms: 0,
            duration_ms: 12,
            findings: 2,
            files_scanned: 1,
          },
        ];
      case "baseline_info":
        return null;
      case "get_scan":
        return findings;
      case "get_provider_config":
        return {
          kind: "openai",
          base_url: "https://api.openai.com/v1",
          model: "test-model",
          has_key: true,
        };
      case "llm_triage":
        return {
          assessment: "likely-false-positive",
          rationale: "静态字符串，未流入危险汇聚点",
        };
      case "llm_triage_stream": {
        const { requestId } = (args ?? {}) as { requestId?: string };
        const delta = listeners.get("llm://delta");
        delta?.({ requestId, text: '{"assessment":"true-' });
        delta?.({ requestId, text: 'positive"}' });
        return '{"assessment":"true-positive"}';
      }
      case "llm_batch_triage":
        return [
          {
            fingerprint: "fp-1",
            assessment: "true-positive",
            rationale: "命中真实危险构造",
            missing: false,
          },
          {
            fingerprint: "fp-2",
            assessment: "uncertain",
            rationale: "缺少上下文",
            missing: false,
          },
        ];
      case "llm_generate_rule":
        return {
          yaml: "rules:\n  - id: custom/no-eval-param\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (return_statement) @match\n",
          valid: true,
          errors: [],
        };
      default:
        return null;
    }
  });
});

describe("App 主链路", () => {
  it("四个 tab 可切换，规则页列出规则并支持启停", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "规则" }));
    const table = await screen.findByTestId("rules-table");
    expect(table).toBeInTheDocument();
    expect(screen.getByText("ts/no-eval")).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: "启用 ts/no-eval" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_rule_enabled", {
        ruleId: "ts/no-eval",
        enabled: false,
      });
    });
  });

  it("设置页加载并保存 Provider 配置，密钥状态可见", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "设置" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_provider_config");
    });
    expect(screen.getByTestId("key-status")).toHaveTextContent("已配置");
    const model = screen.getByLabelText("Model") as HTMLInputElement;
    await waitFor(() => expect(model.value).toBe("test-model"));

    await user.clear(model);
    await user.type(model, "new-model");
    await user.click(screen.getByRole("button", { name: "保存 Provider 配置" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_provider_config", {
        kind: "openai",
        baseUrl: "https://api.openai.com/v1",
        model: "new-model",
      });
    });
  });

  it("分诊面板标注非确定性参考，候选规则可保存", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "选择工作区" }));
    await user.click(screen.getByRole("button", { name: "开始扫描" }));
    const finished = listeners.get("scan://finished");
    await act(async () => {
      finished!({ scan_id: 7, stats: { files_scanned: 1 } });
    });
    await user.click(screen.getByRole("button", { name: "发现" }));
    await user.click(await screen.findByText("src/a.ts:3"));

    // 纪律标注始终可见。
    expect(screen.getByText(/非确定性参考 · 不参与门禁/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "LLM 分诊" }));
    const triage = await screen.findByTestId("triage-result");
    expect(triage.textContent).toContain("likely-false-positive");
    expect(invokeMock).toHaveBeenCalledWith("llm_triage", {
      scanId: 7,
      fingerprint: "fp-1",
    });

    await user.click(screen.getByRole("button", { name: "生成候选规则" }));
    const candidate = await screen.findByTestId("candidate-rule");
    expect(candidate.textContent).toContain("校验通过");
    await user.click(screen.getByRole("button", { name: "保存为自定义规则" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_custom_rule", {
        yaml: expect.stringContaining("custom/no-eval-param"),
      });
    });
  });

  it("流式分诊实时渲染增量，批量分诊列表标注参考纪律", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "选择工作区" }));
    await user.click(screen.getByRole("button", { name: "开始扫描" }));
    const finished = listeners.get("scan://finished");
    await act(async () => {
      finished!({ scan_id: 7, stats: { files_scanned: 1 } });
    });
    await user.click(screen.getByRole("button", { name: "发现" }));
    await user.click(await screen.findByText("src/a.ts:3"));

    await user.click(screen.getByRole("button", { name: "流式分诊" }));
    const stream = await screen.findByTestId("triage-stream");
    await waitFor(() => expect(stream.textContent).toContain("true-positive"));
    expect(invokeMock).toHaveBeenCalledWith("llm_triage_stream", {
      scanId: 7,
      fingerprint: "fp-1",
      requestId: expect.any(String),
    });

    await user.click(screen.getByRole("button", { name: "批量分诊" }));
    const batch = await screen.findByTestId("batch-triage");
    expect(batch.textContent).toContain("true-positive");
    expect(batch.textContent).toContain("非确定性参考 · 不参与门禁");
    expect(invokeMock).toHaveBeenCalledWith("llm_batch_triage", {
      scanId: 7,
      fingerprints: ["fp-1", "fp-2"],
    });
  });

  it("历史页可下钻查看发现详情与污点链", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "历史" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("list_scans");
    });

    await user.click(await screen.findByRole("button", { name: "查看" }));
    const list = await screen.findByTestId("history-findings");
    expect(list.textContent).toContain("src/a.ts:3");
    expect(list.textContent).toContain("metric/long");

    await user.click(screen.getByText("src/a.ts:3"));
    const detail = await screen.findByTestId("history-finding-detail");
    expect(detail.textContent).toContain("eval(x)");
    const flows = screen.getByTestId("history-taint-flows");
    expect(flows.textContent).toContain("第 2 行");
  });

  it("选择工作区 → 扫描 → finished 事件 → 发现列表与过滤", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "选择工作区" }));
    await waitFor(() => {
      expect(screen.getByTestId("workspace-path")).toHaveTextContent("/tmp/ws");
    });

    await user.click(screen.getByRole("button", { name: "开始扫描" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("start_scan", { root: "/tmp/ws" });
    });

    // 触发 finished 事件（模拟 Rust 侧扫描完成）。
    const finished = listeners.get("scan://finished");
    expect(finished).toBeTruthy();
    await act(async () => {
      finished!({
        scan_id: 7,
        stats: { files_scanned: 2, files_total: 2 },
      });
    });

    await user.click(screen.getByRole("button", { name: "发现" }));
    const list = await screen.findByTestId("finding-list");
    expect(list.textContent).toContain("src/a.ts:3");
    expect(list.textContent).toContain("metric/long");

    // 维度过滤：security 只留一条。
    await user.selectOptions(screen.getByLabelText("维度过滤"), "security");
    expect(list.textContent).not.toContain("metric/long");

    // 详情面板。
    await user.click(screen.getByText("src/a.ts:3"));
    expect(screen.getByTestId("match-text")).toHaveTextContent("eval(x)");
    // B6：污点链渲染（source 位置列表）。
    const flows = screen.getByTestId("taint-flows");
    expect(flows.textContent).toContain("第 2 行");
  });
});
