import { useState } from "react";
import { ScanView } from "./views/ScanView";
import { FindingsView } from "./views/FindingsView";
import { RulesView } from "./views/RulesView";
import { HistoryView } from "./views/HistoryView";
import { SettingsView } from "./views/SettingsView";
import { useUpdateCheck } from "./useUpdateCheck";
import { useUpdateStore } from "./stores/updateStore";

type Tab = "scan" | "findings" | "rules" | "history" | "settings";

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "scan", label: "扫描" },
  { id: "findings", label: "发现" },
  { id: "rules", label: "规则" },
  { id: "history", label: "历史" },
  { id: "settings", label: "设置" },
];

export function App() {
  const [tab, setTab] = useState<Tab>("scan");
  const available = useUpdateStore((s) => s.available);
  useUpdateCheck();

  return (
    <main className="app-shell">
      <nav className="tabs" aria-label="主导航">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={tab === t.id ? "active" : ""}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
        <span className="app-meta">
          {available && (
            <button
              type="button"
              className="update-badge"
              data-testid="update-badge"
              title={`发现新版本 v${available.version}，点击前往设置更新`}
              aria-label={`发现新版本 v${available.version}，点击前往设置更新`}
              onClick={() => setTab("settings")}
            >
              <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                <path d="M8 1.6 2.6 7h2.7v5.2h5.4V7h2.7L8 1.6Z" fill="currentColor" />
                <path d="M3 13.4h10v1.2H3z" fill="currentColor" />
              </svg>
            </button>
          )}
          <span className="app-version" data-testid="app-version">
            v{__APP_VERSION__}
          </span>
        </span>
      </nav>
      {tab === "scan" && <ScanView />}
      {tab === "findings" && <FindingsView />}
      {tab === "rules" && <RulesView />}
      {tab === "history" && <HistoryView />}
      {tab === "settings" && <SettingsView />}
    </main>
  );
}
