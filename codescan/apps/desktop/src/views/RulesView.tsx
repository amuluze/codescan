/** 规则页：内置 + 自定义规则目录、启停、校验错误、自定义规则编辑与测试。 */
import { useEffect, useState } from "react";
import {
  api,
  type RuleInfo,
  type RulePackInfo,
  type RuleTestHit,
} from "../platform";
import { DirectoryPicker } from "../components/DirectoryPicker";
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

const TEST_LANGUAGES = [
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "python",
  "java",
  "go",
  "rust",
] as const;

export function RulesView() {
  const { rules, customError, loading, load, toggle } = useRulesStore();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [dirty, setDirty] = useState(false);
  const [editorErrors, setEditorErrors] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [testSample, setTestSample] = useState("eval(userInput);\n");
  const [testLanguage, setTestLanguage] =
    useState<(typeof TEST_LANGUAGES)[number]>("typescript");
  const [testHits, setTestHits] = useState<RuleTestHit[]>([]);
  const [testError, setTestError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
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
  const editing = editorOpen && (selected ? Boolean(selected.editable) : true);

  const openRule = async (rule: RuleInfo) => {
    if (dirty && !window.confirm("放弃未保存的规则修改？")) return;
    setSelectedId(rule.id);
    setEditorOpen(Boolean(rule.editable));
    setEditorErrors([]);
    setNotice(null);
    setTestHits([]);
    setTestError(null);
    if (rule.editable) {
      const raw = await api.getCustomRules().catch(() => null);
      setDraft(raw ?? NEW_RULE_TEMPLATE);
      setDirty(false);
    }
  };

  const startNewRule = async () => {
    if (dirty && !window.confirm("放弃未保存的规则修改？")) return;
    setSelectedId(null);
    setEditorOpen(true);
    setDraft(NEW_RULE_TEMPLATE);
    setDirty(true);
    setEditorErrors([]);
    setNotice(null);
    setTestHits([]);
    setTestError(null);
  };

  const validate = async () => {
    setEditorErrors([]);
    setNotice(null);
    const errors = await api.validateCustomRules(draft).catch((error) => [
      String(error),
    ]);
    setEditorErrors(errors);
    setNotice(errors.length === 0 ? "规则校验通过" : null);
  };

  const save = async () => {
    setSaving(true);
    setNotice(null);
    setEditorErrors([]);
    try {
      await api.saveCustomRules(draft);
      setDirty(false);
      setNotice("自定义规则已保存");
      await load();
    } catch (error) {
      setEditorErrors(String(error).split("\n").filter(Boolean));
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setTesting(true);
    setTestHits([]);
    setTestError(null);
    try {
      const hits = await api.testCustomRule(draft, testSample, testLanguage);
      setTestHits(hits);
    } catch (error) {
      setTestError(String(error));
    } finally {
      setTesting(false);
    }
  };

  if (loading) return <p className="muted">加载规则…</p>;

  return (
    <section className="rules-view" aria-label="规则">
      {customError && (
        <p role="alert" data-testid="custom-rules-error">
          自定义规则加载失败：
          {customError}
        </p>
      )}
      <div className="row">
        <button className="ghost btn-sm" onClick={() => void startNewRule()}>
          新建自定义规则
        </button>
      </div>
      <table className="rules-table">
        <thead>
          <tr>
            <th>启用</th>
            <th>id</th>
            <th>语言</th>
            <th>维度</th>
            <th>严重级</th>
            <th>来源</th>
            <th>说明</th>
          </tr>
        </thead>
        <tbody data-testid="rules-table">
          {rules.map((r) => (
            <tr
              key={r.id}
              className={`${r.error ? "rule-error-row" : ""} ${
                selectedId === r.id ? "selected" : ""
              }`}
              onClick={() => void openRule(r)}
            >
              <td>
                <input
                  type="checkbox"
                  aria-label={`启用 ${r.id}`}
                  checked={r.enabled}
                  disabled={Boolean(r.error)}
                  onClick={(event) => event.stopPropagation()}
                  onChange={(e) => void toggle(r.id, e.target.checked)}
                />
              </td>
              <td>{r.id}</td>
              <td>{r.language}</td>
              <td>{r.dimension}</td>
              <td>{r.severity}</td>
              <td>{r.origin ?? ""}</td>
              <td>
                {r.error ? (
                  <span className="degraded" title={r.error}>
                    {r.message}
                  </span>
                ) : (
                  r.message
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <section className="detail-section" data-testid="rule-packs">
        <div className="row">
          <h3>规则包</h3>
          <button
            className="ghost btn-sm"
            onClick={() => setPackPickerOpen(true)}
            disabled={packBusy}
          >
            安装规则包
          </button>
        </div>
        {packError && (
          <p role="alert" data-testid="rule-pack-error">
            {packError}
          </p>
        )}
        {packs.length === 0 ? (
          <p className="muted">尚未安装规则包。规则包目录需包含 pack.yaml。</p>
        ) : (
          <table className="rules-table">
            <thead>
              <tr>
                <th>id</th>
                <th>版本</th>
                <th>规则数</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {packs.map((pack) => (
                <tr key={`${pack.id}@${pack.version}`}>
                  <td>{pack.id}</td>
                  <td>{pack.version}</td>
                  <td>{pack.rules}</td>
                  <td>
                    {pack.error ? (
                      <span className="degraded" title={pack.error}>
                        加载失败
                      </span>
                    ) : pack.signed_by ? (
                      <span title={pack.signed_by}>已安装 · 已签名</span>
                    ) : (
                      "已安装"
                    )}
                  </td>
                  <td>
                    <button
                      className="ghost btn-sm"
                      disabled={packBusy}
                      onClick={() => void removePack(pack)}
                    >
                      卸载
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {selected && !editing && (
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

      {editing && (
        <section className="detail-section" data-testid="rule-editor">
          <h3>{selected ? `编辑 ${selected.id}` : "新建自定义规则"}</h3>
          <p className="muted">
            编辑的是完整 custom.yaml；保存前会按当前启停状态编译校验。内置规则只读。
          </p>
          <textarea
            className="rule-editor"
            aria-label="自定义规则 YAML"
            value={draft}
            onChange={(event) => {
              setDraft(event.target.value);
              setDirty(true);
              setEditorErrors([]);
              setNotice(null);
            }}
            spellCheck={false}
          />
          <div className="row">
            <button
              className="ghost btn-sm"
              onClick={() => void validate()}
              disabled={saving || testing}
            >
              校验
            </button>
            <button
              className="primary btn-sm"
              onClick={() => void save()}
              disabled={saving || testing || !dirty}
            >
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
          {editorErrors.length > 0 && (
            <ul className="rule-errors" role="alert" data-testid="rule-editor-errors">
              {editorErrors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          )}
          {notice && <p role="status">{notice}</p>}

          <h4>测试当前自定义规则</h4>
          <div className="row">
            <select
              aria-label="测试语言"
              value={testLanguage}
              onChange={(event) =>
                setTestLanguage(event.target.value as (typeof TEST_LANGUAGES)[number])
              }
            >
              {TEST_LANGUAGES.map((language) => (
                <option key={language} value={language}>
                  {language}
                </option>
              ))}
            </select>
            <button
              className="ghost btn-sm"
              onClick={() => void test()}
              disabled={testing || saving}
            >
              {testing ? "测试中…" : "运行测试"}
            </button>
          </div>
          <textarea
            className="rule-editor rule-sample"
            aria-label="规则测试样例"
            value={testSample}
            onChange={(event) => setTestSample(event.target.value)}
            spellCheck={false}
          />
          {testError && <p role="alert">{testError}</p>}
          {testHits.length > 0 && (
            <ul data-testid="rule-test-hits">
              {testHits.map((hit, index) => (
                <li key={`${hit.rule_id}-${hit.line}-${hit.column}-${index}`}>
                  {hit.rule_id} @ {hit.line}:{hit.column} — {hit.message}
                  <code>{hit.match_text}</code>
                </li>
              ))}
            </ul>
          )}
          {!testError && testHits.length === 0 && testing === false && (
            <p className="muted">运行测试后这里显示实际命中。</p>
          )}
        </section>
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
