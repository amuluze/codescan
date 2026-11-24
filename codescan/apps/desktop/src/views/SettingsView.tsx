/** 设置页：LLM Provider Profile（多 Profile · 兼容 OpenAI / Anthropic）、API Key（只写）与应用更新。 */
import { useEffect, useState } from "react";
import { useLlmStore } from "../stores/llmStore";
import { useUpdateStore } from "../stores/updateStore";

type EditorState = {
  /** null = 新建（保存后落库并回填 id，便于继续配置 Key）。 */
  id: string | null;
  name: string;
  kind: string;
  baseUrl: string;
  model: string;
};

const EMPTY_EDITOR: EditorState = {
  id: null,
  name: "",
  kind: "openai",
  baseUrl: "",
  model: "",
};

export function SettingsView() {
  const {
    providers,
    loadProviders,
    saveProfile,
    deleteProfile,
    setActiveProfile,
    setProfileKey,
    clearProfileKey,
    testing,
    testResult,
    testError,
    testConnection,
  } = useLlmStore();
  const {
    available: updateInfo,
    checking: updateChecking,
    installing: updateInstalling,
    message: updateMessage,
    error: updateError,
    check: checkUpdate,
    install: installUpdate,
  } = useUpdateStore();
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  useEffect(() => {
    loadProviders().catch((e) => setMessage(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const editingId = editor?.id ?? null;
  const editingProfile = providers?.profiles.find((p) => p.id === editingId) ?? null;

  const save = async () => {
    if (!editor) return;
    try {
      const id = await saveProfile(editor);
      setEditor({ ...editor, id });
      setMessage(null);
    } catch (e) {
      setMessage(String(e));
    }
  };

  const saveKey = async () => {
    if (!editingId || !apiKey.trim()) return;
    try {
      await setProfileKey(editingId, apiKey.trim());
      setApiKey("");
      setMessage("API Key 已保存");
    } catch (e) {
      setMessage(String(e));
    }
  };

  const removeProfile = async (id: string) => {
    try {
      await deleteProfile(id);
      if (editor?.id === id) setEditor(null);
      setMessage(null);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setConfirmDeleteId(null);
    }
  };

  return (
    <section className="page settings-view" aria-label="设置">
      <header className="page-head">
        <div>
          <h2>设置</h2>
          <p className="page-sub">
            LLM 辅助层配置。密钥、扫描历史与基线均只保存在本地数据根。
          </p>
        </div>
      </header>

      {message && (
        <p role="status" className="notice">
          {message}
        </p>
      )}

      <div className="settings-stack">
        <section className="card">
          <header className="card-head">
            <div>
              <h3>模型 Provider</h3>
              <p className="card-sub">
                可保存多个 Provider Profile；密钥按 Profile 独立写入 Rust
                独占的 secrets 表，无内置遥测，请求只发往该 Profile 的 Base URL。
                带「默认」标记的 Profile 是分诊与审查的缺省选择。
              </p>
            </div>
          </header>

          {providers && providers.profiles.length > 0 && (
            <table className="provider-table" data-testid="provider-table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>协议</th>
                  <th>模型</th>
                  <th>Base URL</th>
                  <th>Key</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {providers.profiles.map((p) => (
                  <tr key={p.id}>
                    <td>
                      {p.name}
                      {providers.active === p.id && (
                        <span className="dim-badge" title="活动 Profile：分诊与审查的缺省选择">
                          默认
                        </span>
                      )}
                    </td>
                    <td>{p.kind === "openai" ? "OpenAI" : "Anthropic"}</td>
                    <td className="mono-cell">{p.model}</td>
                    <td className="mono-cell">{p.base_url}</td>
                    <td>
                      <span className={`status-chip ${p.has_key ? "ok" : "muted-chip"}`}>
                        {p.has_key ? "已配置" : "未配置"}
                      </span>
                    </td>
                    <td>
                      <div className="table-actions">
                        {providers.active !== p.id && (
                          <button
                            className="ghost btn-sm"
                            onClick={() => void setActiveProfile(p.id)}
                          >
                            设为默认
                          </button>
                        )}
                        <button
                          className="ghost btn-sm"
                          onClick={() =>
                            setEditor({
                              id: p.id,
                              name: p.name,
                              kind: p.kind,
                              baseUrl: p.base_url,
                              model: p.model,
                            })
                          }
                        >
                          编辑
                        </button>
                        <button
                          className="ghost btn-sm"
                          disabled={testing}
                          onClick={() => void testConnection(p.id)}
                        >
                          测试
                        </button>
                        {confirmDeleteId === p.id ? (
                          <>
                            <button className="danger btn-sm" onClick={() => void removeProfile(p.id)}>
                              确认删除
                            </button>
                            <button
                              className="ghost btn-sm"
                              onClick={() => setConfirmDeleteId(null)}
                            >
                              取消
                            </button>
                          </>
                        ) : (
                          <button className="ghost btn-sm" onClick={() => setConfirmDeleteId(p.id)}>
                            删除
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {providers && providers.profiles.length === 0 && (
            <p className="card-sub">尚无 Provider Profile；新建一个以启用 LLM 辅助能力。</p>
          )}

          {editor && (
            <div className="profile-editor" data-testid="profile-editor">
              <div className="form-grid">
                <label className="field">
                  <span className="field-label">名称</span>
                  <input
                    aria-label="Profile 名称"
                    value={editor.name}
                    onChange={(e) => setEditor({ ...editor, name: e.target.value })}
                    placeholder="例如：OpenAI 主力 / 智谱 GLM"
                  />
                </label>
                <label className="field">
                  <span className="field-label">Provider</span>
                  <select
                    aria-label="Provider"
                    value={editor.kind}
                    onChange={(e) => setEditor({ ...editor, kind: e.target.value })}
                  >
                    <option value="openai">OpenAI-compatible</option>
                    <option value="anthropic">Anthropic-compatible</option>
                  </select>
                </label>
                <label className="field">
                  <span className="field-label">Base URL</span>
                  <input
                    aria-label="Base URL"
                    value={editor.baseUrl}
                    onChange={(e) => setEditor({ ...editor, baseUrl: e.target.value })}
                    placeholder="https://api.openai.com/v1"
                  />
                </label>
                <label className="field field-wide">
                  <span className="field-label">Model</span>
                  <input
                    aria-label="Model"
                    value={editor.model}
                    onChange={(e) => setEditor({ ...editor, model: e.target.value })}
                    placeholder="模型名，必填（例如 gpt-4o-mini / glm-4.6）"
                  />
                </label>
              </div>
              <footer className="card-actions">
                <button
                  className="primary"
                  data-testid="profile-save"
                  onClick={() => void save()}
                >
                  保存 Profile
                </button>
                <button className="ghost" onClick={() => setEditor(null)}>
                  取消
                </button>
              </footer>
              {editingId ? (
                <div className="key-block">
                  <div className="key-head">
                    <span className="field-label">API Key</span>
                    <span
                      className={`status-chip ${editingProfile?.has_key ? "ok" : "muted-chip"}`}
                      data-testid="key-status"
                    >
                      {editingProfile?.has_key ? "已配置" : "未配置"}
                    </span>
                  </div>
                  <div className="key-row">
                    <input
                      aria-label="API Key"
                      type="password"
                      className="key-input"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder="sk-…"
                      autoComplete="off"
                    />
                    <button
                      className="primary"
                      onClick={() => void saveKey()}
                      disabled={!apiKey.trim()}
                    >
                      保存 Key
                    </button>
                    <button
                      className="ghost"
                      onClick={() => void clearProfileKey(editingId)}
                      disabled={!editingProfile?.has_key}
                    >
                      清除 Key
                    </button>
                  </div>
                  <p className="key-note">仅写入 Rust 独占的 SQLite secrets 表；命令面不提供明文回读。</p>
                </div>
              ) : (
                <p className="key-note">保存 Profile 后即可设置该 Profile 的 API Key。</p>
              )}
            </div>
          )}
          {!editor && (
            <footer className="card-actions">
              <button
                className="ghost"
                data-testid="profile-new"
                onClick={() => setEditor({ ...EMPTY_EDITOR })}
              >
                新建 Profile
              </button>
            </footer>
          )}

          {testResult && (
            <p role="status" className="notice success" data-testid="provider-test-ok">
              连接成功 · {testResult.kind} · {testResult.model} · {testResult.latency_ms} ms
            </p>
          )}
          {testError && (
            <p role="alert" className="notice failure" data-testid="provider-test-error">
              {testError}
            </p>
          )}
        </section>

        <section className="card">
          <header className="card-head">
            <div>
              <h3>应用更新</h3>
              <p className="card-sub">经 GitHub Releases 检查新版本（应用启动时与每 6 小时自动检查）；更新包经 minisign 签名校验后替换安装并重启。</p>
            </div>
          </header>
          <div className="key-row">
            <button
              className="ghost"
              data-testid="update-check"
              onClick={() => void checkUpdate()}
              disabled={updateChecking || updateInstalling}
            >
              {updateChecking ? "检查中…" : "检查更新"}
            </button>
            {updateInfo && (
              <button
                className="primary"
                data-testid="update-install"
                onClick={() => void installUpdate()}
                disabled={updateInstalling}
              >
                {updateInstalling ? "下载安装中…" : `下载并安装 v${updateInfo.version}`}
              </button>
            )}
          </div>
          {updateInfo && updateInfo.notes && <p className="key-note">{updateInfo.notes}</p>}
          {updateMessage && (
            <p role="status" className="notice" data-testid="update-status">
              {updateMessage}
            </p>
          )}
          {updateError && (
            <p role="alert" className="notice failure" data-testid="update-error">
              {updateError}
            </p>
          )}
        </section>

        <section className="card card-note">
          <h3>安全边界</h3>
          <ul className="note-list">
            <li>LLM 输出仅作展示参考，永不进入门禁与基线；只有经校验并显式保存的规则才影响后续扫描。</li>
            <li>密钥不出现在日志、错误消息、发现或任何导出物中。</li>
            <li>扫描本身只读工作区；历史、基线与规则写入 <code>~/.codescan/</code>。</li>
          </ul>
        </section>
      </div>
    </section>
  );
}
