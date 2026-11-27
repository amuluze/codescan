/** 自定义规则编辑器：新增单条规则追加保存，编辑既有规则保存完整 custom.yaml。 */
import { useState } from "react";
import { api, type RuleTestHit } from "../platform";

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

type Props = {
  title: string;
  initialYaml: string;
  appendOnly: boolean;
  onSaved: () => Promise<void> | void;
  onClose: () => void;
};

export function RuleEditorDialog({
  title,
  initialYaml,
  appendOnly,
  onSaved,
  onClose,
}: Props) {
  const [draft, setDraft] = useState(initialYaml);
  const [dirty, setDirty] = useState(appendOnly);
  const [editorErrors, setEditorErrors] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [testSample, setTestSample] = useState("eval(userInput);\n");
  const [testLanguage, setTestLanguage] =
    useState<(typeof TEST_LANGUAGES)[number]>("typescript");
  const [testHits, setTestHits] = useState<RuleTestHit[]>([]);
  const [testError, setTestError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  const close = () => {
    if (dirty && !window.confirm("放弃未保存的规则修改？")) return;
    onClose();
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
      if (appendOnly) {
        await api.saveCustomRule(draft);
      } else {
        await api.saveCustomRules(draft);
      }
      setDirty(false);
      setNotice("自定义规则已保存");
      if (appendOnly) {
        await onSaved();
        onClose();
      } else {
        void onSaved();
      }
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

  return (
    <div className="modal-backdrop" role="presentation">
      <div
        className="modal rule-editor-modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        data-testid="rule-editor"
      >
        <header className="modal-head">
          <h3>{title}</h3>
          <button
            className="ghost"
            onClick={close}
            aria-label="关闭规则编辑器"
            disabled={saving}
          >
            ✕
          </button>
        </header>

        <p className="muted">
          {appendOnly
            ? "新增规则会追加到 custom.yaml，不覆盖已有自定义规则；保存前会编译校验。"
            : "编辑的是完整 custom.yaml；保存前会按当前启停状态编译校验。内置规则只读。"}
        </p>

        <div className="rule-editor-body">
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
            autoFocus
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
              className="ghost btn-sm"
              onClick={() => void test()}
              disabled={testing || saving}
            >
              {testing ? "测试中…" : "运行测试"}
            </button>
          </div>
          {editorErrors.length > 0 && (
            <ul
              className="rule-errors"
              role="alert"
              data-testid="rule-editor-errors"
            >
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
        </div>

        <footer className="modal-foot">
          <div className="row">
            <button className="ghost" onClick={close} disabled={saving}>
              取消
            </button>
            <button
              className="primary"
              onClick={() => void save()}
              disabled={saving || testing || !dirty}
            >
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
