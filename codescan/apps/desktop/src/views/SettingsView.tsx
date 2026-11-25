/** 设置页：LLM Provider Profile（多 Profile · 兼容 OpenAI / Anthropic）、API Key（只写）与应用更新。 */
import { useEffect, useState } from "react";
import { useLlmStore } from "../stores/llmStore";
import { useUpdateStore } from "../stores/updateStore";
import {
  api,
  type ReviewSkillFileInfo,
  type ReviewSkillInfo,
} from "../platform";
import { SkillEditorDialog } from "../components/SkillEditorDialog";

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

type PendingEditorAction =
  | { kind: "open"; target: EditorState }
  | { kind: "close" };

/** Base URL 必须是可解析的 http(s) 地址；与后端 ProviderConfig::validate 同口径。 */
const isHttpUrl = (value: string): boolean => {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
};

type ProfileFieldErrors = {
  name?: string;
  baseUrl?: string;
  model?: string;
};

/** 客户端校验：与后端一致的三项必填 + Base URL 协议。 */
const profileFieldErrors = (editor: EditorState): ProfileFieldErrors => {
  const errors: ProfileFieldErrors = {};
  if (!editor.name.trim()) errors.name = "名称为必填项";
  if (!editor.baseUrl.trim()) {
    errors.baseUrl = "Base URL 为必填项";
  } else if (!isHttpUrl(editor.baseUrl.trim())) {
    errors.baseUrl = "必须以 http:// 或 https:// 开头";
  }
  if (!editor.model.trim()) errors.model = "Model 为必填项";
  return errors;
};

/** 展示实际请求 URL：OpenAI 追加 /chat/completions，Anthropic 追加 /v1/messages。 */
const endpointPreview = (editor: EditorState): string => {
  const base = editor.baseUrl.trim().replace(/\/+$/, "");
  if (!base || !isHttpUrl(base)) return "—";
  return editor.kind === "anthropic"
    ? `${base}/v1/messages`
    : `${base}/chat/completions`;
};

/** 常见重复路径：Base URL 已包含协议追加的路径时提示。 */
const endpointWarning = (editor: EditorState): string | null => {
  const base = editor.baseUrl.trim().replace(/\/+$/, "");
  if (!base || !isHttpUrl(base)) return null;
  if (editor.kind === "anthropic") {
    if (/\/v1\/messages$/i.test(base)) {
      return "Base URL 已包含 /v1/messages；实际请求会再追加一次";
    }
    if (/\/v1$/i.test(base)) {
      return "Anthropic 会自动追加 /v1/messages；Base URL 请勿以 /v1 结尾";
    }
  } else if (/\/chat\/completions$/i.test(base)) {
    return "Base URL 已包含 /chat/completions；实际请求会再追加一次";
  }
  return null;
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
  const [baseline, setBaseline] = useState<EditorState | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [savedNotice, setSavedNotice] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingEditorAction | null>(null);
  const [savingProfile, setSavingProfile] = useState(false);
  const [keyBusy, setKeyBusy] = useState(false);
  const [activatingId, setActivatingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [testedName, setTestedName] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [skillInfo, setSkillInfo] = useState<ReviewSkillInfo | null>(null);
  const [skillError, setSkillError] = useState<string | null>(null);
  const [skillMessage, setSkillMessage] = useState<string | null>(null);
  const [editingSkill, setEditingSkill] = useState<ReviewSkillFileInfo | null>(null);
  const [skillSaving, setSkillSaving] = useState(false);

  useEffect(() => {
    loadProviders().catch((e) => setMessage(String(e)));
    api
      .llmReviewSkillInfo()
      .then(setSkillInfo)
      .catch((e) => setSkillError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const editingId = editor?.id ?? null;
  const editingProfile = providers?.profiles.find((p) => p.id === editingId) ?? null;
  /** 字段或 Key 草稿与打开时不同即视为未保存；切换/关闭前必须显式放弃。 */
  const dirty = Boolean(
    editor &&
      baseline &&
      (editor.name !== baseline.name ||
        editor.kind !== baseline.kind ||
        editor.baseUrl !== baseline.baseUrl ||
        editor.model !== baseline.model ||
        apiKey !== ""),
  );
  const fieldErrors = editor ? profileFieldErrors(editor) : {};
  const invalid = Object.keys(fieldErrors).length > 0;
  const missingFields = editor
    ? [
        !editor.name.trim() ? "名称" : null,
        !editor.baseUrl.trim() ? "Base URL" : null,
        !editor.model.trim() ? "Model" : null,
      ].filter((value): value is string => value !== null)
    : [];
  const endpoint = editor ? endpointPreview(editor) : "";
  const endpointWarningText = editor ? endpointWarning(editor) : null;

  const closeEditor = () => {
    setEditor(null);
    setBaseline(null);
    setApiKey("");
    setProfileError(null);
    setSavedNotice(null);
    setPendingAction(null);
  };

  const openEditor = (target: EditorState) => {
    setEditor(target);
    setBaseline(target);
    setApiKey("");
    setProfileError(null);
    setSavedNotice(null);
    setPendingAction(null);
    setMessage(null);
  };

  const startEditing = (target: EditorState) => {
    if (editor?.id && editor.id === target.id) return;
    if (dirty) {
      setPendingAction({ kind: "open", target });
      return;
    }
    openEditor(target);
  };

  const requestClose = () => {
    if (dirty) {
      setPendingAction({ kind: "close" });
      return;
    }
    closeEditor();
  };

  const confirmDiscard = () => {
    if (!pendingAction) return;
    if (pendingAction.kind === "close") closeEditor();
    else openEditor(pendingAction.target);
  };

  const changeEditor = (patch: Partial<EditorState>) => {
    setEditor((current) => (current ? { ...current, ...patch } : current));
    setProfileError(null);
    setSavedNotice(null);
  };

  const save = async () => {
    if (!editor || savingProfile || invalid) return;
    const wasNew = !editor.id;
    const normalized: EditorState = {
      ...editor,
      name: editor.name.trim(),
      baseUrl: editor.baseUrl.trim(),
      model: editor.model.trim(),
    };
    setSavingProfile(true);
    setProfileError(null);
    try {
      const id = await saveProfile(normalized);
      const saved = { ...normalized, id };
      setBaseline(saved);
      if (wasNew) {
        // 新建后保留编辑器，让用户继续配置 Key；此时已有 id，按钮变为「关闭」。
        setEditor(saved);
        setSavedNotice("Profile 已保存；可继续配置 API Key，完成后点击关闭。");
      } else {
        closeEditor();
        setMessage(`已保存 Provider Profile：${saved.name}`);
      }
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setSavingProfile(false);
    }
  };

  const saveKey = async () => {
    if (!editingId || keyBusy || !apiKey.trim()) return;
    setKeyBusy(true);
    setProfileError(null);
    try {
      await setProfileKey(editingId, apiKey.trim());
      setApiKey("");
      setSavedNotice(`API Key 已保存（${editingProfile?.name ?? "当前 Profile"}）`);
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setKeyBusy(false);
    }
  };

  const clearKey = async () => {
    if (!editingId || keyBusy) return;
    setKeyBusy(true);
    setProfileError(null);
    try {
      await clearProfileKey(editingId);
      setApiKey("");
      setSavedNotice(`API Key 已清除（${editingProfile?.name ?? "当前 Profile"}）`);
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setKeyBusy(false);
    }
  };

  const activate = async (p: { id: string; name: string }) => {
    if (activatingId) return;
    setActivatingId(p.id);
    try {
      await setActiveProfile(p.id);
      setMessage(`默认 Provider 已切换为：${p.name}`);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setActivatingId(null);
    }
  };

  const removeProfile = async (p: { id: string; name: string }) => {
    if (deletingId) return;
    setDeletingId(p.id);
    try {
      await deleteProfile(p.id);
      if (editor?.id === p.id) closeEditor();
      setMessage(`已删除 Provider Profile：${p.name}`);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setConfirmDeleteId(null);
      setDeletingId(null);
    }
  };

  const testProfile = async (p: { id: string; name: string }) => {
    setTestedName(p.name);
    await testConnection(p.id);
  };

  /** 保存/恢复后就地更新列表；打开中的编辑器同步为新内容。 */
  const applySkillFile = (updated: ReviewSkillFileInfo) => {
    setSkillInfo((current) =>
      current
        ? {
            ...current,
            files: current.files.map((file) =>
              file.name === updated.name ? updated : file,
            ),
          }
        : current,
    );
    setEditingSkill((current) =>
      current && current.name === updated.name ? updated : current,
    );
  };

  const saveSkill = async (content: string) => {
    if (!editingSkill || skillSaving) return;
    setSkillSaving(true);
    setSkillError(null);
    try {
      const updated = await api.saveLlmReviewSkill(editingSkill.name, content);
      applySkillFile(updated);
      setEditingSkill(null);
      setSkillMessage(`已保存 ${updated.name}；后续审查使用覆写内容`);
    } catch (e) {
      setSkillError(String(e));
    } finally {
      setSkillSaving(false);
    }
  };

  const resetSkill = async (name: string) => {
    if (skillSaving) return;
    setSkillSaving(true);
    setSkillError(null);
    try {
      const updated = await api.resetLlmReviewSkill(name);
      applySkillFile(updated);
      setSkillMessage(`已恢复 ${name} 的内置默认内容`);
    } catch (e) {
      setSkillError(String(e));
    } finally {
      setSkillSaving(false);
    }
  };

  const customizedSkills = skillInfo?.files.filter((file) => file.customized).length ?? 0;

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
                  <tr
                    key={p.id}
                    className={editor?.id === p.id ? "editing" : undefined}
                  >
                    <td data-label="名称">
                      {p.name}
                      {providers.active === p.id && (
                        <span className="dim-badge" title="活动 Profile：分诊与审查的缺省选择">
                          默认
                        </span>
                      )}
                    </td>
                    <td data-label="协议">{p.kind === "openai" ? "OpenAI" : "Anthropic"}</td>
                    <td data-label="模型" className="mono-cell" title={p.model}>
                      {p.model}
                    </td>
                    <td data-label="Base URL" className="mono-cell url-cell" title={p.base_url}>
                      {p.base_url}
                    </td>
                    <td data-label="Key">
                      <span className={`status-chip ${p.has_key ? "ok" : "muted-chip"}`}>
                        {p.has_key ? "已配置" : "未配置"}
                      </span>
                    </td>
                    <td data-label="操作">
                      <div className="table-actions">
                        {providers.active !== p.id && (
                          <button
                            className="ghost btn-sm"
                            disabled={activatingId !== null || deletingId !== null}
                            onClick={() => void activate(p)}
                          >
                            {activatingId === p.id ? "切换中…" : "设为默认"}
                          </button>
                        )}
                        <button
                          className="ghost btn-sm"
                          disabled={savingProfile}
                          onClick={() =>
                            startEditing({
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
                          disabled={testing || deletingId !== null}
                          onClick={() => void testProfile(p)}
                        >
                          {testing && testedName === p.name ? "测试中…" : "测试"}
                        </button>
                        {confirmDeleteId === p.id ? (
                          <>
                            <button
                              className="danger btn-sm"
                              disabled={deletingId === p.id}
                              onClick={() => void removeProfile(p)}
                            >
                              {deletingId === p.id ? "删除中…" : "确认删除"}
                            </button>
                            <button
                              className="ghost btn-sm"
                              disabled={deletingId === p.id}
                              onClick={() => setConfirmDeleteId(null)}
                            >
                              取消
                            </button>
                          </>
                        ) : (
                          <button
                            className="ghost btn-sm"
                            disabled={deletingId !== null}
                            onClick={() => setConfirmDeleteId(p.id)}
                          >
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
              <div className="profile-editor-head">
                <h4>
                  {editor.id
                    ? `编辑 Provider Profile：${baseline?.name ?? editor.name}`
                    : "新建 Provider Profile"}
                </h4>
                {editingId && providers?.active === editingId && (
                  <span className="dim-badge">默认</span>
                )}
              </div>

              {savedNotice && (
                <p role="status" className="notice success profile-notice" data-testid="profile-notice">
                  {savedNotice}
                </p>
              )}
              {profileError && (
                <p role="alert" className="notice failure profile-notice" data-testid="profile-error">
                  {profileError}
                </p>
              )}
              {pendingAction && (
                <div className="profile-discard" role="alert">
                  <span>当前编辑有未保存的修改，切换或关闭将丢失。</span>
                  <div className="row">
                    <button className="danger btn-sm" onClick={confirmDiscard}>
                      放弃修改
                    </button>
                    <button className="ghost btn-sm" onClick={() => setPendingAction(null)}>
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
                    value={editor.name}
                    onChange={(e) => changeEditor({ name: e.target.value })}
                    placeholder="例如：OpenAI 主力 / 智谱 GLM"
                    autoFocus={!editor.id}
                  />
                </label>
                <label className="field">
                  <span className="field-label">协议</span>
                  <select
                    aria-label="Provider"
                    value={editor.kind}
                    onChange={(e) => changeEditor({ kind: e.target.value })}
                  >
                    <option value="openai">OpenAI-compatible</option>
                    <option value="anthropic">Anthropic-compatible</option>
                  </select>
                </label>
                <label
                  className={`field field-wide${
                    editor.baseUrl.trim() && fieldErrors.baseUrl ? " field-invalid" : ""
                  }`}
                >
                  <span className="field-label">Base URL</span>
                  <input
                    aria-label="Base URL"
                    value={editor.baseUrl}
                    onChange={(e) => changeEditor({ baseUrl: e.target.value })}
                    placeholder="https://api.openai.com/v1"
                  />
                  {editor.baseUrl.trim() && fieldErrors.baseUrl && (
                    <span className="field-error">{fieldErrors.baseUrl}</span>
                  )}
                </label>
                <label className="field field-wide">
                  <span className="field-label">Model</span>
                  <input
                    aria-label="Model"
                    value={editor.model}
                    onChange={(e) => changeEditor({ model: e.target.value })}
                    placeholder="模型名，必填（例如 gpt-4o-mini / glm-4.6）"
                  />
                </label>
              </div>

              <p className="profile-endpoint">
                <span>
                  实际请求：<code>{endpoint}</code>
                </span>
                {endpointWarningText && (
                  <span className="profile-endpoint-warn">{endpointWarningText}</span>
                )}
              </p>

              <footer className="card-actions">
                <button
                  className="primary"
                  data-testid="profile-save"
                  onClick={() => void save()}
                  disabled={savingProfile || invalid || pendingAction !== null}
                >
                  {savingProfile ? "保存中…" : "保存 Profile"}
                </button>
                <button
                  className="ghost"
                  onClick={requestClose}
                  disabled={savingProfile || pendingAction !== null}
                >
                  {editor.id ? "关闭" : "取消"}
                </button>
                {missingFields.length > 0 && (
                  <span className="profile-hint">
                    请补全：{missingFields.join("、")}
                  </span>
                )}
              </footer>

              {editingId ? (
                <div className="key-block">
                  <div className="key-head">
                    <span className="field-label">
                      API Key · {editingProfile?.name ?? baseline?.name ?? "当前 Profile"}
                    </span>
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
                      onChange={(e) => {
                        setApiKey(e.target.value);
                        setSavedNotice(null);
                        setProfileError(null);
                      }}
                      placeholder="sk-…"
                      autoComplete="off"
                    />
                    <button
                      className="primary"
                      onClick={() => void saveKey()}
                      disabled={!apiKey.trim() || keyBusy}
                    >
                      {keyBusy ? "处理中…" : "保存 Key"}
                    </button>
                    <button
                      className="ghost"
                      onClick={() => void clearKey()}
                      disabled={!editingProfile?.has_key || keyBusy}
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
                onClick={() => startEditing({ ...EMPTY_EDITOR })}
              >
                新建 Profile
              </button>
            </footer>
          )}

          {testResult && (
            <p role="status" className="notice success" data-testid="provider-test-ok">
              连接成功 · {testedName ? `${testedName} · ` : ""}
              {testResult.kind} · {testResult.model} · {testResult.latency_ms} ms
            </p>
          )}
          {testError && (
            <p role="alert" className="notice failure" data-testid="provider-test-error">
              {testedName ? `${testedName}：` : ""}
              {testError}
            </p>
          )}
        </section>

        <section className="card skill-card" data-testid="skill-card">
          <header className="card-head">
            <div>
              <h3>LLM 审查内置 Skill</h3>
              <p className="card-sub">
                源码审查使用随应用分发的技能包（{skillInfo?.version ?? "llm-review/v1"}）；
                可按文件查看并保存本地覆写。覆写只影响模型可见内容，不改变源码不可信声明、
                JSON 输出校验与「不参与门禁」边界。
              </p>
            </div>
            {customizedSkills > 0 && (
              <span className="dim-badge">{customizedSkills} 个覆写</span>
            )}
          </header>

          {skillError && !editingSkill && (
            <p role="alert" className="notice failure" data-testid="skill-error">
              {skillError}
            </p>
          )}
          {skillMessage && (
            <p role="status" className="notice" data-testid="skill-status">
              {skillMessage}
            </p>
          )}

          {skillInfo ? (
            <table className="provider-table skill-table" data-testid="skill-table">
              <thead>
                <tr>
                  <th>文件</th>
                  <th>用途</th>
                  <th>大小</th>
                  <th>状态</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {skillInfo.files.map((file) => (
                  <tr key={file.name}>
                    <td className="mono-cell">{file.name}</td>
                    <td>{file.title}</td>
                    <td className="mono-cell">{file.bytes} B</td>
                    <td>
                      <span
                        className={`status-chip ${file.customized ? "ok" : "muted-chip"}`}
                      >
                        {file.customized ? "已覆写" : "内置"}
                      </span>
                    </td>
                    <td>
                      <div className="table-actions">
                        <button
                          className="ghost btn-sm"
                          aria-label={`编辑 ${file.name}`}
                          onClick={() => {
                            setSkillError(null);
                            setEditingSkill(file);
                          }}
                        >
                          编辑
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            !skillError && <p className="card-sub">加载中…</p>
          )}

          <p className="key-note">
            覆写保存在 <code>~/.codescan/skills/llm-review.json</code>；单文件 ≤{" "}
            {Math.round((skillInfo?.max_file_bytes ?? 16 * 1024) / 1024)} KiB，合计 ≤{" "}
            {Math.round((skillInfo?.max_total_bytes ?? 32 * 1024) / 1024)} KiB。
            与内置正文相同的保存视为恢复默认。
          </p>

          <SkillEditorDialog
            file={editingSkill}
            maxBytes={skillInfo?.max_file_bytes ?? 16 * 1024}
            saving={skillSaving}
            error={skillError}
            onSave={(content) => void saveSkill(content)}
            onReset={() => editingSkill && void resetSkill(editingSkill.name)}
            onClose={() => {
              setEditingSkill(null);
              setSkillError(null);
            }}
          />
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
