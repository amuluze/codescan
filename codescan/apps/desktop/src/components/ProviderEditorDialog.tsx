/** Provider Profile 编辑弹窗：Profile 字段与 API Key 在同一次保存中提交。 */
import { useEffect, useState, type FormEvent } from "react";
import {
  EMPTY_PROVIDER_EDITOR,
  endpointPreview,
  endpointWarning,
  profileFieldErrors,
  type ProviderEditorState,
} from "../providerProfile";

export type ProviderSavePayload = {
  profile: ProviderEditorState;
  /** null = 不修改现有 Key。 */
  apiKey: string | null;
  clearKey: boolean;
};

type Props = {
  target: ProviderEditorState | null;
  keyConfigured: boolean;
  saving: boolean;
  error: string | null;
  onSave: (payload: ProviderSavePayload) => void;
  onClose: () => void;
};

export function ProviderEditorDialog({
  target,
  keyConfigured,
  saving,
  error,
  onSave,
  onClose,
}: Props) {
  const [draft, setDraft] = useState<ProviderEditorState>(
    target ?? EMPTY_PROVIDER_EDITOR,
  );
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);

  // 打开另一个 Profile / 新建时重置草稿与 Key 输入。
  useEffect(() => {
    if (!target) return;
    setDraft(target);
    setApiKey("");
    setClearKey(false);
    setConfirmDiscard(false);
  }, [target]);

  if (!target) return null;

  const dirty =
    draft.name !== target.name ||
    draft.kind !== target.kind ||
    draft.baseUrl !== target.baseUrl ||
    draft.model !== target.model ||
    apiKey !== "" ||
    (clearKey && keyConfigured);

  const fieldErrors = profileFieldErrors(draft);
  const invalid = Object.keys(fieldErrors).length > 0;
  const missingFields = [
    !draft.name.trim() ? "名称" : null,
    !draft.baseUrl.trim() ? "Base URL" : null,
    !draft.model.trim() ? "Model" : null,
  ].filter((value): value is string => value !== null);
  const endpoint = endpointPreview(draft);
  const warning = endpointWarning(draft);

  const requestClose = () => {
    if (dirty) {
      setConfirmDiscard(true);
      return;
    }
    onClose();
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (saving || invalid || confirmDiscard) return;
    onSave({
      profile: {
        ...draft,
        name: draft.name.trim(),
        baseUrl: draft.baseUrl.trim(),
        model: draft.model.trim(),
      },
      apiKey: clearKey ? null : apiKey.trim() || null,
      clearKey,
    });
  };

  return (
    <div className="modal-backdrop" role="presentation">
      <form
        className="modal provider-editor-modal"
        role="dialog"
        aria-modal="true"
        aria-label={target.id ? "编辑 Provider Profile" : "新建 Provider Profile"}
        onSubmit={submit}
      >
        <header className="modal-head">
          <div>
            <h3>{target.id ? "编辑 Provider Profile" : "新建 Provider Profile"}</h3>
            <p className="card-sub">
              {target.id
                ? `当前 Profile：${target.name}`
                : "Profile 与 API Key 会在同一次保存中提交。"}
            </p>
          </div>
          <button
            type="button"
            className="ghost"
            aria-label="关闭 Provider 编辑器"
            onClick={requestClose}
            disabled={saving}
          >
            ✕
          </button>
        </header>

        {error && (
          <p role="alert" className="notice failure profile-notice" data-testid="profile-error">
            {error}
          </p>
        )}
        {confirmDiscard && (
          <div className="profile-discard" role="alert">
            <span>当前编辑有未保存的修改，关闭将丢失。</span>
            <div className="row">
              <button type="button" className="danger btn-sm" onClick={onClose}>
                放弃修改
              </button>
              <button
                type="button"
                className="ghost btn-sm"
                onClick={() => setConfirmDiscard(false)}
              >
                继续编辑
              </button>
            </div>
          </div>
        )}

        <div className="form-grid">
          <label className="field">
            <span className="field-label">名称</span>
            <input
              aria-label="Profile 名称"
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              placeholder="例如：OpenAI 主力 / 智谱 GLM"
              autoFocus={!draft.id}
              disabled={saving}
            />
          </label>
          <label className="field">
            <span className="field-label">协议</span>
            <select
              aria-label="Provider"
              value={draft.kind}
              onChange={(e) => setDraft({ ...draft, kind: e.target.value })}
              disabled={saving}
            >
              <option value="openai">OpenAI-compatible</option>
              <option value="anthropic">Anthropic-compatible</option>
            </select>
          </label>
          <label
            className={`field field-wide${
              draft.baseUrl.trim() && fieldErrors.baseUrl ? " field-invalid" : ""
            }`}
          >
            <span className="field-label">Base URL</span>
            <input
              aria-label="Base URL"
              value={draft.baseUrl}
              onChange={(e) => setDraft({ ...draft, baseUrl: e.target.value })}
              placeholder="https://api.openai.com/v1"
              disabled={saving}
            />
            {draft.baseUrl.trim() && fieldErrors.baseUrl && (
              <span className="field-error">{fieldErrors.baseUrl}</span>
            )}
          </label>
          <label className="field field-wide">
            <span className="field-label">Model</span>
            <input
              aria-label="Model"
              value={draft.model}
              onChange={(e) => setDraft({ ...draft, model: e.target.value })}
              placeholder="模型名，必填（例如 gpt-4o-mini / glm-4.6）"
              disabled={saving}
            />
          </label>
        </div>

        <p className="profile-endpoint">
          <span>
            实际请求：<code>{endpoint}</code>
          </span>
          {warning && <span className="profile-endpoint-warn">{warning}</span>}
        </p>

        <div className="key-block">
          <div className="key-head">
            <span className="field-label">
              API Key{keyConfigured ? " · 已配置" : "（可选）"}
            </span>
            <span
              className={`status-chip ${keyConfigured ? "ok" : "muted-chip"}`}
              data-testid="key-status"
            >
              {keyConfigured ? "已配置" : "未配置"}
            </span>
          </div>
          <div className="key-row">
            <input
              aria-label="API Key"
              type="password"
              className="key-input"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={
                keyConfigured
                  ? "留空保持不变；输入新 Key 可替换"
                  : "sk-…（可稍后配置）"
              }
              autoComplete="off"
              disabled={clearKey || saving}
            />
            {keyConfigured && (
              <label className="inline-check muted provider-key-clear">
                <input
                  type="checkbox"
                  aria-label="清除已保存的 Key"
                  checked={clearKey}
                  onChange={(e) => setClearKey(e.target.checked)}
                  disabled={saving}
                />
                清除已保存的 Key
              </label>
            )}
          </div>
          <p className="key-note">
            与 Profile 一起保存；留空表示保持原 Key。密钥只写入 Rust 独占的 SQLite secrets 表。
          </p>
        </div>

        <footer className="modal-foot">
          <span className="profile-hint">
            {missingFields.length > 0
              ? `请补全：${missingFields.join("、")}`
              : ""}
          </span>
          <div className="row">
            <button
              type="button"
              className="ghost"
              onClick={requestClose}
              disabled={saving || confirmDiscard}
            >
              取消
            </button>
            <button
              type="submit"
              className="primary"
              data-testid="provider-save"
              disabled={saving || invalid || confirmDiscard}
            >
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
        </footer>
      </form>
    </div>
  );
}
