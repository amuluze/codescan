/** A8 主链路：tab 导航、规则列表/启停、扫描触发 → 进度事件 → 发现过滤。 */
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
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
  fingerprint: "fp",
  ...overrides,
});

const findings = [
  finding({ fingerprint: "fp-1" }),
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
  invokeMock.mockImplementation(async (cmd: string) => {
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
        return [];
      case "baseline_info":
        return null;
      case "get_scan":
        return findings;
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
        scanId: 7,
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
  });
});
