import { useState } from "react";
import { ScanView } from "./views/ScanView";
import { FindingsView } from "./views/FindingsView";
import { RulesView } from "./views/RulesView";
import { HistoryView } from "./views/HistoryView";
import { SettingsView } from "./views/SettingsView";

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
      </nav>
      {tab === "scan" && <ScanView />}
      {tab === "findings" && <FindingsView />}
      {tab === "rules" && <RulesView />}
      {tab === "history" && <HistoryView />}
      {tab === "settings" && <SettingsView />}
    </main>
  );
}
