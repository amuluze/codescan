/** 规则页：内置 + 自定义规则目录、启停、校验错误、自定义规则编辑与测试。 */
import { useEffect, useState } from "react";
import { api, type RuleInfo, type RulePackInfo } from "../platform";
import { DirectoryPicker } from "../components/DirectoryPicker";
import { RuleEditorDialog } from "../components/RuleEditorDialog";
import { RulePacksSection } from "../components/RulePacksSection";
import { RuleTable } from "../components/RuleTable";
import { useRulesStore } from "../stores/rulesStore";

const NEW_RULE_TEMPLATE = `rules:
  - id: custom/my-rule
    language: typescript
    dimension: quality
    severity: info
    message: 描述出现的构造
    pattern: |
      (call_expression
        function: (identifier) @match (#eq? @match "eval")) @match
`;

export function RulesView() {
  const { rules, customError, loading, load, toggle } = useRulesStore();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<{
    title: string;
    yaml: string;
    appendOnly: boolean;
  } | null>(null);
  const [packs, setPacks] = useState<RulePackInfo[]>([]);
  const [packError, setPackError] = useState<string | null>(null);
  const [packPickerOpen, setPackPickerOpen] = useState(false);
  const [packBusy, setPackBusy] = useState(false);

  const loadPacks = async () => {
    try {
      setPacks(await api.listRulePacks());
      setPackError(null);
    } catch (error) {
      setPackError(String(error));
    }
  };

  useEffect(() => {
    void load();
    void loadPacks();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const installPack = async (sourceDir: string) => {
    setPackPickerOpen(false);
    setPackBusy(true);
    setPackError(null);
    try {
      await api.installRulePack(sourceDir);
      await load();
      await loadPacks();
    } catch (error) {
      setPackError(String(error));
    } finally {
      setPackBusy(false);
    }
  };

  const removePack = async (pack: RulePackInfo) => {
    setPackBusy(true);
    setPackError(null);
    try {
      await api.removeRulePack(pack.id, pack.version);
      await load();
      await loadPacks();
    } catch (error) {
      setPackError(String(error));
    } finally {
      setPackBusy(false);
    }
  };

  const selected = rules.find((rule) => rule.id === selectedId) ?? null;

  const openRule = async (rule: RuleInfo) => {
    setSelectedId(rule.id);
    if (!rule.editable) {
      setEditor(null);
      return;
    }
    const raw = await api.getCustomRules().catch(() => null);
    setEditor({
      title: `编辑 ${rule.id}`,
      yaml: raw ?? NEW_RULE_TEMPLATE,
      appendOnly: false,
    });
  };

  const startNewRule = () => {
    setSelectedId(null);
    setEditor({
      title: "新建自定义规则",
      yaml: NEW_RULE_TEMPLATE,
      appendOnly: true,
    });
  };

  const closeEditor = () => setEditor(null);

  if (loading && rules.length === 0) return <p className="muted">加载规则…</p>;

  return (
    <section className="rules-view" aria-label="规则">
      {customError && (
        <p role="alert" data-testid="custom-rules-error">
          自定义规则加载失败：
          {customError}
        </p>
      )}
      <div className="row">
        <button className="ghost btn-sm" onClick={startNewRule}>
          新建自定义规则
        </button>
      </div>
      <RuleTable
        rules={rules}
        selectedId={selectedId}
        onOpen={(rule) => void openRule(rule)}
        onToggle={toggle}
      />

      <RulePacksSection
        packs={packs}
        error={packError}
        busy={packBusy}
        onInstall={() => setPackPickerOpen(true)}
        onRemove={(pack) => void removePack(pack)}
      />

      {selected && !selected.editable && (
        <section className="detail-section" data-testid="rule-detail">
          <h3>{selected.id}</h3>
          <p className="muted">
            {selected.language} · {selected.dimension} · {selected.severity}
            {selected.error ? " · 规则错误" : ""}
          </p>
          <pre className="code-block">
            <code>{selected.source}</code>
          </pre>
        </section>
      )}

      {editor && (
        <RuleEditorDialog
          title={editor.title}
          initialYaml={editor.yaml}
          appendOnly={editor.appendOnly}
          onSaved={load}
          onClose={closeEditor}
        />
      )}
      {packPickerOpen && (
        <DirectoryPicker
          title="选择规则包目录"
          confirmLabel="安装规则包"
          onSelect={(path) => void installPack(path)}
          onClose={() => setPackPickerOpen(false)}
        />
      )}
    </section>
  );
}
