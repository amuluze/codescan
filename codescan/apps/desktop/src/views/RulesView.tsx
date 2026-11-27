/** 规则页：内置 + 自定义规则目录、启停、校验错误、自定义规则编辑与测试。 */
import { useEffect, useState } from "react";
import { api, type RuleInfo } from "../platform";
import { DirectoryPicker } from "../components/DirectoryPicker";
import { RuleEditorDialog } from "../components/RuleEditorDialog";
import { useRulesStore } from "../stores/rulesStore";
import {
  RulePacksSection,
  RuleReadOnlyDetail,
  RulesTable,
  useRulePacks,
} from "./RulesView.parts";

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
  const packs = useRulePacks(load);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<{
    title: string;
    yaml: string;
    appendOnly: boolean;
  } | null>(null);

  useEffect(() => {
    void load();
    void packs.loadPacks();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
      <RulesTable
        rules={rules}
        selectedId={selectedId}
        onOpen={(rule) => void openRule(rule)}
        onToggle={toggle}
      />

      <RulePacksSection model={packs} />

      {selected && !selected.editable && <RuleReadOnlyDetail rule={selected} />}

      {editor && (
        <RuleEditorDialog
          title={editor.title}
          initialYaml={editor.yaml}
          appendOnly={editor.appendOnly}
          onSaved={load}
          onClose={() => setEditor(null)}
        />
      )}
      {packs.packPickerOpen && (
        <DirectoryPicker
          title="选择规则包目录"
          confirmLabel="安装规则包"
          onSelect={(path) => void packs.installPack(path)}
          onClose={() => packs.setPackPickerOpen(false)}
        />
      )}
    </section>
  );
}
