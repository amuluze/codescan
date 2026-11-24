/** A8 主链路：tab 导航、规则列表/启停、扫描触发 → 进度事件 → 发现过滤。 */
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { updater } from "./platform";
import { useLlmStore } from "./stores/llmStore";
import { useScanStore } from "./stores/scanStore";
import { useUpdateStore } from "./stores/updateStore";
import { UPDATE_CHECK_INTERVAL_MS } from "./useUpdateCheck";
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
    deleteScan: (scanId: number) => platform.invoke("delete_scan", { scanId }),
    setBaseline: (scanId: number) => platform.invoke("set_baseline", { scanId }),
    gateDiff: (scanId: number) => platform.invoke("gate_diff", { scanId }),
    baselineInfo: () => platform.invoke("baseline_info"),
    exportSarif: (scanId: number, dest: string) =>
      platform.invoke("export_sarif", { scanId, dest }),
    exportReport: (destDir: string, base: string, markdown: string, json: string) =>
      platform.invoke("export_report", { destDir, base, markdown, json }),
    listDirectory: (path?: string) =>
      platform.invoke("list_directory", { path: path ?? null }),
    getProviderConfig: () => platform.invoke("get_provider_config"),
    setProviderConfig: (kind: string, baseUrl: string, model: string) =>
      platform.invoke("set_provider_config", { kind, baseUrl, model }),
    setApiKey: (key: string) => platform.invoke("set_api_key", { key }),
    clearApiKey: () => platform.invoke("clear_api_key"),
    testProviderConnection: () => platform.invoke("test_provider_connection"),
    llmReviewWorkspace: (
      root: string,
      maxFiles?: number,
      offset?: number,
      requestId?: string,
    ) =>
      platform.invoke("llm_review_workspace", {
        root,
        maxFiles: maxFiles ?? null,
        offset: offset ?? 0,
        requestId: requestId ?? null,
      }),
    llmTriage: (scanId: number, fingerprint: string) =>
      platform.invoke("llm_triage", { scanId, fingerprint }),
    llmTriageStream: (scanId: number, fingerprint: string, requestId: string) =>
      platform.invoke("llm_triage_stream", { scanId, fingerprint, requestId }),
    llmBatchTriage: (scanId: number, fingerprints: string[]) =>
      platform.invoke("llm_batch_triage", { scanId, fingerprints }),
    llmGenerateRule: (scanId: number, fingerprint: string) =>
      platform.invoke("llm_generate_rule", { scanId, fingerprint }),
    llmCandidateRegression: (
      scanId: number,
      fingerprint: string,
      candidateYaml: string,
      saveFixture: boolean,
    ) =>
      platform.invoke("llm_candidate_regression", {
        scanId,
        fingerprint,
        candidateYaml,
        saveFixture,
      }),
    saveCustomRule: (yaml: string) => platform.invoke("save_custom_rule", { yaml }),
  };
  const updater = {
    check: vi.fn(async () => null),
    installAndRelaunch: vi.fn(async () => {}),
  };
  return { platform, api, updater };
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
  vi.mocked(updater.check).mockReset().mockResolvedValue(null);
  vi.mocked(updater.installAndRelaunch).mockReset().mockResolvedValue(undefined);
  useUpdateStore.setState({
    available: null,
    checking: false,
    installing: false,
    message: null,
    error: null,
  });
  useScanStore.setState({
    phase: "idle",
    workspace: null,
    recentRoots: [],
    progress: null,
    error: null,
    lastScanId: null,
    stats: null,
    findings: [],
    filter: { dimension: "all", severity: "all", query: "" },
    selected: null,
  });
  useLlmStore.setState({
    provider: null,
    testing: false,
    testResult: null,
    testError: null,
    review: null,
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
      case "delete_scan":
        return null;
      case "list_directory": {
        const path = (args?.path as string | undefined) ?? "/tmp/ws";
        if (path === "/tmp/other") {
          return {
            path: "/tmp/other",
            parent: "/tmp",
            entries: [],
          };
        }
        return {
          path: "/tmp/ws",
          parent: "/",
          entries: [{ name: "workspace-sub", path: "/tmp/ws/sub" }],
        };
      }
      case "get_provider_config":
        return {
          kind: "openai",
          base_url: "https://api.openai.com/v1",
          model: "test-model",
          has_key: true,
        };
      case "test_provider_connection":
        return { kind: "openai", model: "test-model", latency_ms: 87 };
      case "llm_review_workspace": {
        const offset = (args?.offset as number | undefined) ?? 0;
        return {
          files_reviewed: offset === 0 ? 2 : 1,
          candidate_files: 3,
          next_offset: offset === 0 ? 2 : 3,
          total_bytes: 2048,
          model: "test-model",
          dropped: 1,
          suggestions: [
            {
              path: "src/a.ts",
              line: 3,
              severity: "warning",
              category: "business-logic",
              title: "缺少订单归属校验",
              detail: "更新前未校验订单是否属于当前用户。",
              confidence: "medium",
            },
          ],
        };
      }
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
      case "llm_candidate_regression":
        return {
          valid: true,
          pass: false,
          errors: [],
          expected: [4],
          actual: [{ line: 6, message: "m" }],
          failures: ["漏报：规则 custom/no-eval-param 未命中期望行 4"],
          saved_paths: ["/tmp/.codescan/rules/tests/custom_no-eval-param/a.ts"],
        };
      case "export_report":
        return null;
      default:
        return null;
    }
  });
});

describe("App 主链路", () => {
  it("顶部导航右侧展示应用版本（语义化版本号）", () => {
    render(<App />);
    expect(screen.getByTestId("app-version").textContent).toMatch(/^v\d+\.\d+\.\d+$/);
  });

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

  it("点击选择工作区必定打开选择器；列表失败时在模态内呈现错误", async () => {
    const user = userEvent.setup();
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: InvokeArgs,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
      if (cmd === "list_directory") throw new Error("目录不可读");
      return base(cmd, args);
    });
    render(<App />);

    await user.click(screen.getByRole("button", { name: "选择工作区" }));
    const picker = await screen.findByTestId("dir-picker");
    expect(picker).toBeInTheDocument();
    const alert = await screen.findByTestId("dir-picker-error");
    expect(alert.textContent).toContain("目录不可读");
    expect(screen.getByTestId("dir-picker-confirm")).toBeDisabled();
    expect(screen.getByTestId("workspace-path")).toHaveTextContent("未选择");
  });

  it("目录选择器可进入子目录并确认选择", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "选择工作区" }));
    await screen.findByTestId("dir-picker");
    await user.click(await screen.findByText("workspace-sub"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("list_directory", { path: "/tmp/ws/sub" });
    });
    await user.click(screen.getByTestId("dir-picker-confirm"));
    await waitFor(() => {
      expect(screen.getByTestId("workspace-path")).toHaveTextContent("/tmp/ws");
    });
  });

  it("扫描页从历史快速选择工作区：去重、最新在前，选中即可扫描", async () => {
    const user = userEvent.setup();
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: InvokeArgs,
    ) => Promise<unknown>;
    const scan = (root: string, id: number, startedAtMs: number) => ({
      id,
      root,
      engine_version: "0.1.0",
      started_at_ms: startedAtMs,
      duration_ms: 5,
      findings: 1,
      files_scanned: 2,
    });
    invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
      if (cmd === "list_scans") {
        return [
          scan("/tmp/old", 1, 100),
          scan("/tmp/new", 2, 300),
          scan("/tmp/old", 3, 200),
        ];
      }
      return base(cmd, args);
    });
    render(<App />);

    const select = (await screen.findByTestId("recent-workspace")) as HTMLSelectElement;
    // 占位项在前；历史按最近扫描去重排序。
    expect(Array.from(select.options).map((o) => o.value)).toEqual([
      "",
      "/tmp/new",
      "/tmp/old",
    ]);

    await user.selectOptions(select, "/tmp/new");
    expect(screen.getByTestId("workspace-path")).toHaveTextContent("/tmp/new");
    await user.click(screen.getByRole("button", { name: "开始扫描" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("start_scan", { root: "/tmp/new" });
    });
  });

  it("无扫描历史时不显示快捷选择，提示保持目录浏览", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: InvokeArgs,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
      if (cmd === "list_scans") return [];
      return base(cmd, args);
    });
    render(<App />);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("list_scans");
    });
    expect(screen.queryByTestId("recent-workspace")).toBeNull();
    expect(screen.getByText("尚未选择工作区。点击「选择工作区」浏览本地目录后开始。")).toBeInTheDocument();
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

  it("候选规则隔离回归展示通过/未通过与期望实际行", async () => {
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

    await user.click(screen.getByRole("button", { name: "生成候选规则" }));
    await screen.findByTestId("candidate-rule");

    // 仅勾选夹具时才请求保存；结果展示未通过、期望/实际行与失败原因。
    await user.click(screen.getByRole("checkbox", { name: "保存回归夹具" }));
    await user.click(screen.getByRole("button", { name: "运行回归验证" }));
    const result = await screen.findByTestId("regression-result");
    expect(result.textContent).toContain("未通过");
    expect(result.textContent).toContain("期望行：4");
    expect(result.textContent).toContain("实际行：6");
    expect(result.textContent).toContain("漏报");
    expect(result.textContent).toContain("夹具已保存");
    expect(invokeMock).toHaveBeenCalledWith("llm_candidate_regression", {
      scanId: 7,
      fingerprint: "fp-1",
      candidateYaml: expect.stringContaining("custom/no-eval-param"),
      saveFixture: true,
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
    const picker = await screen.findByTestId("dir-picker");
    expect(picker).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByTestId("dir-picker-current")).toHaveTextContent("/tmp/ws");
    });
    await user.click(screen.getByTestId("dir-picker-confirm"));
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
    expect(screen.getByTestId("finding-summary").textContent).toContain("共 2");

    // 维度过滤：security 只留一条。
    await user.selectOptions(screen.getByLabelText("维度过滤"), "security");
    expect(list.textContent).not.toContain("metric/long");

    // 无结果时给出空状态引导而非空白列表。
    await user.type(screen.getByLabelText("搜索"), "zzz-no-match");
    expect(list.textContent).toContain("无匹配发现");
    await user.click(screen.getByRole("button", { name: "清除过滤" }));

    // 详情面板。
    await user.click(screen.getByText("src/a.ts:3"));
    expect(screen.getByTestId("match-text")).toHaveTextContent("eval(x)");
    // B6：污点链渲染（source 位置列表）。
    const flows = screen.getByTestId("taint-flows");
    expect(flows.textContent).toContain("第 2 行");
  });

  it("设置页测试连接成功时展示模型与延迟", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "设置" }));
    await user.click(await screen.findByRole("button", { name: "测试连接" }));
    const ok = await screen.findByTestId("provider-test-ok");
    expect(ok).toHaveTextContent("连接成功");
    expect(ok).toHaveTextContent("test-model");
    expect(ok).toHaveTextContent("87 ms");
  });

  it("设置页测试连接失败时展示可读错误", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "设置" }));
    await screen.findByTestId("key-status");
    invokeMock.mockRejectedValueOnce(new Error("Provider 返回 401：Unauthorized"));
    await user.click(screen.getByRole("button", { name: "测试连接" }));
    const err = await screen.findByTestId("provider-test-error");
    expect(err).toHaveTextContent("401");
  });

  it("设置页检查更新：有更新可安装，失败时错误可见（开发模式降级）", async () => {
    const user = userEvent.setup();
    // 应用启动即后台检查一次，手动检查同样命中该结果。
    vi.mocked(updater.check).mockResolvedValue({
      version: "0.2.0",
      notes: "修复与改进",
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: "设置" }));
    await user.click(await screen.findByTestId("update-check"));
    expect(await screen.findByTestId("update-install")).toHaveTextContent(
      "下载并安装 v0.2.0",
    );
    await user.click(screen.getByTestId("update-install"));
    await waitFor(() => expect(updater.installAndRelaunch).toHaveBeenCalled());

    // 检查失败（开发模式无已发布 release 属预期）：错误展示，不阻断页面。
    vi.mocked(updater.check).mockRejectedValue(new Error("更新端点不可达"));
    await user.click(screen.getByTestId("update-check"));
    const err = await screen.findByTestId("update-error");
    expect(err).toHaveTextContent("更新端点不可达");
  });

  it("后台检查发现新版本时，顶栏版本号前显示升级提示并跳转设置页", async () => {
    const user = userEvent.setup();
    vi.mocked(updater.check).mockResolvedValue({
      version: "9.9.9",
      notes: "全新版本",
    });
    render(<App />);

    const badge = await screen.findByTestId("update-badge");
    expect(badge).toHaveAttribute("aria-label", expect.stringContaining("9.9.9"));
    // 图标位于版本号之前。
    const version = screen.getByTestId("app-version");
    expect(
      badge.compareDocumentPosition(version) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    await user.click(badge);
    expect(await screen.findByTestId("update-install")).toHaveTextContent(
      "下载并安装 v9.9.9",
    );
  });

  it("后台检查失败时不打扰用户：无升级提示、无错误提示", async () => {
    vi.mocked(updater.check).mockRejectedValue(new Error("更新端点不可达"));
    render(<App />);
    await waitFor(() => expect(updater.check).toHaveBeenCalled());
    expect(screen.queryByTestId("update-badge")).toBeNull();
    expect(screen.queryByTestId("update-error")).toBeNull();
  });

  it("更新按固定间隔复查", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<App />);
      await vi.waitFor(() => expect(updater.check).toHaveBeenCalledTimes(1));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(UPDATE_CHECK_INTERVAL_MS);
      });
      expect(updater.check).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("批量分诊、流式分诊与 LLM 分诊输出支持一键复制", async () => {
    useScanStore.setState({
      phase: "finished",
      lastScanId: 7,
      findings: [
        finding({ fingerprint: "fp-1" }),
        finding({
          fingerprint: "fp-2",
          rule_id: "metric/long",
          dimension: "quality",
          severity: "info",
          path: "src/b.py",
        }),
      ],
      selected: null,
    });
    const user = userEvent.setup();
    // 注意：userEvent.setup() 会替换 navigator.clipboard，mock 必须在其后注入。
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: "发现" }));

    // 批量分诊：逐条复制。
    await user.click(screen.getByRole("button", { name: "批量分诊" }));
    await user.click(await screen.findByRole("button", { name: "复制批量分诊第 1 条" }));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("true-positive"));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("src/a.ts:3"));

    // LLM 分诊：复制评估与理由。
    await user.click(screen.getByRole("button", { name: "LLM 分诊" }));
    await user.click(await screen.findByRole("button", { name: "复制分诊建议" }));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("likely-false-positive"));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("静态字符串"));

    // 流式分诊：复制已接收的完整输出。
    await user.click(screen.getByRole("button", { name: "流式分诊" }));
    await user.click(await screen.findByRole("button", { name: "复制流式分诊输出" }));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("true-positive"));
  });

  it("历史记录删除需要二次确认", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "历史" }));
    await screen.findByTestId("history-table");
    await user.click(screen.getByRole("button", { name: "删除" }));
    // 第一次点击只进入确认态，不调用后端。
    expect(screen.queryByRole("button", { name: "删除" })).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith("delete_scan", expect.anything());
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_scan", { scanId: 7 }),
    );
  });

  it("扫描页可发起 LLM 源码审查并自动分页覆盖全部候选", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const user = userEvent.setup();
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    const list = await screen.findByTestId("review-suggestions");
    expect(list.textContent).toContain("缺少订单归属校验");
    expect(list.textContent).toContain("src/a.ts:3");

    // 每条建议可单独复制（含证据与置信度）；列表可筛选，按钮以路径定位而非序号。
    await user.click(screen.getByRole("button", { name: "复制审查结果：src/a.ts:3" }));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("缺少订单归属校验"));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("src/a.ts:3"));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("置信度：中"));

    expect(invokeMock).toHaveBeenCalledWith("llm_review_workspace", {
      root: "/tmp/ws",
      maxFiles: 24,
      offset: 0,
      requestId: expect.any(String),
    });

    // 自动续审：无需手动点击，携带游标直至覆盖全部候选。
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("llm_review_workspace", {
        root: "/tmp/ws",
        maxFiles: 24,
        offset: 2,
        requestId: expect.any(String),
      }),
    );
    expect(await screen.findByRole("button", { name: "已审查全部候选" })).toBeDisabled();
    expect(screen.getByText(/已审查 3 \/ 3 个可审查文件/)).toBeTruthy();
    // 两页返回同一条建议：跨页去重后只保留一条。
    // 统计卡片与列表一致：跨页去重后只计 1 条（警告级）。
    expect(screen.getByTestId("review-stats").textContent).toBe("1建议0错误1警告0提示");
    expect(screen.getByText(/过滤 2 条无效/)).toBeTruthy();
  });

  it("自动续审中途失败：保留已审查结果并可从游标恢复", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
      if (cmd === "llm_review_workspace" && ((args?.offset as number) ?? 0) > 0) {
        throw new Error("网络错误");
      }
      return base?.(cmd, args);
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    // 首页结果保留，错误可见，可从游标恢复。
    expect(await screen.findByText(/已审查 2 \/ 3 个可审查文件/)).toBeTruthy();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("网络错误");
    expect(screen.getByRole("button", { name: "继续审查" })).toBeEnabled();
  });

  it("审查中「重置进度」中止自动续审并清空结果", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "llm_review_workspace") {
        return new Promise(() => {}); // 首页挂起，模拟长时间请求
      }
      return base?.(cmd, args);
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    expect(screen.getByRole("button", { name: "审查中…" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "重置进度" }));
    expect(screen.getByRole("button", { name: "开始审查" })).toBeEnabled();
    // 中止后不再发起下一页请求。
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "llm_review_workspace")).toHaveLength(1);
  });

  it("审查建议可按维度筛选并显示计数，可清除过滤", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const user = userEvent.setup();
    render(<App />);
    // 自动审查产生建议后验证筛选行为。
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    await screen.findByTestId("review-suggestions");
    expect(screen.getByTestId("review-filter-count").textContent).toContain("1 / 1");
    // mock 建议是 business-logic：按「质量」筛选应无匹配。
    await user.click(screen.getByRole("combobox", { name: "审查维度过滤" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "审查维度过滤" }), "quality");
    expect(screen.getByTestId("review-filter-count").textContent).toContain("0 / 1");
    expect(screen.getByText(/当前筛选下无匹配建议/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "清除过滤" }));
    expect(screen.getByTestId("review-filter-count").textContent).toContain("1 / 1");
  });

  it("审查建议可导出为 Markdown 与 JSON 文件", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    await screen.findByTestId("review-suggestions");
    await user.click(screen.getByTestId("review-export"));
    expect(screen.getByText("选择审查结果导出目录")).toBeTruthy();
    // 目录列表加载完成后确认按钮才可用；确认目录后调用 export_report。
    const confirm = screen.getByRole("button", { name: "导出到此目录" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_report",
        expect.objectContaining({
          destDir: "/tmp/ws",
          base: "codescan-review",
          markdown: expect.stringContaining("LLM 源码审查建议"),
          json: expect.stringContaining("缺少订单归属校验"),
        }),
      ),
    );
    // 完成提示以对话框呈现，含落盘文件名与目录；可关闭。
    expect(await screen.findByText("导出完成")).toBeTruthy();
    expect(screen.getByTestId("notice-message").textContent).toContain("codescan-review.md");
    expect(screen.getByTestId("notice-message").textContent).toContain("/tmp/ws");
    await user.click(screen.getByTestId("notice-dismiss"));
    expect(screen.queryByText("导出完成")).toBeNull();
  });

  it("发现页可按当前筛选导出发现为 Markdown 与 JSON", async () => {
    const user = userEvent.setup();
    useScanStore.setState({
      lastScanId: 7,
      findings,
      filter: { dimension: "all", severity: "all", query: "src/a.ts" },
      selected: null,
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: "发现" }));
    await screen.findByTestId("finding-list");
    expect(screen.getByText(/显示 1 条/)).toBeTruthy();

    await user.click(screen.getByTestId("findings-export"));
    const confirm = screen.getByRole("button", { name: "导出到此目录" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_report",
        expect.objectContaining({
          destDir: "/tmp/ws",
          base: "codescan-findings-7",
          markdown: expect.stringContaining("fp-1"),
          json: expect.stringContaining('"fingerprint": "fp-1"'),
        }),
      ),
    );
    // 导出跟随当前筛选：被过滤掉的 fp-2 不得出现在内容里。
    const call = invokeMock.mock.calls.find(([cmd]) => cmd === "export_report");
    expect(String(call?.[1]?.json)).not.toContain("fp-2");
    expect(await screen.findByText("导出完成")).toBeTruthy();
    expect(screen.getByTestId("notice-message").textContent).toContain("codescan-findings-7.md");
    await user.click(screen.getByTestId("notice-dismiss"));
  });

  it("导出失败时以对话框展示可读错误", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(async (cmd: string, args?: InvokeArgs) => {
      if (cmd === "export_report") throw new Error("目录不可写");
      return base?.(cmd, args);
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    await screen.findByTestId("review-suggestions");
    await user.click(screen.getByTestId("review-export"));
    const confirm = screen.getByRole("button", { name: "导出到此目录" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);

    expect(await screen.findByText("导出失败")).toBeTruthy();
    expect(screen.getByTestId("notice-message").textContent).toContain("目录不可写");
    await user.click(screen.getByTestId("notice-dismiss"));
    expect(screen.queryByText("导出失败")).toBeNull();
  });

  it("导出进行中按钮显示进度并禁用", async () => {
    useScanStore.setState({ workspace: "/tmp/ws" });
    const base = invokeMock.getMockImplementation();
    const gate = Promise.withResolvers<void>();
    invokeMock.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "export_report") return gate.promise;
      return base?.(cmd, args);
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "开始审查" }));
    await screen.findByTestId("review-suggestions");
    await user.click(screen.getByTestId("review-export"));
    const confirm = screen.getByRole("button", { name: "导出到此目录" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);

    const button = screen.getByTestId("review-export");
    expect(button).toBeDisabled();
    expect(button.textContent).toContain("导出中…");
    await act(async () => {
      gate.resolve();
    });
    expect(await screen.findByText("导出完成")).toBeTruthy();
  });
});
