/** 设置页：LLM Provider Profile（多 Profile · 兼容 OpenAI / Anthropic）、API Key（只写）与应用更新。 */
import { useEffect, useState } from "react";
import { useLlmStore } from "../stores/llmStore";
import { useUpdateStore } from "../stores/updateStore";
import {
  api,
  type ProviderProfileInfo,
  type ReviewSkillFileInfo,
  type ReviewSkillInfo,
} from "../platform";
import { SkillEditorDialog } from "../components/SkillEditorDialog";
import {
  ProviderEditorDialog,
  type ProviderSavePayload,
} from "../components/ProviderEditorDialog";
import {
  EMPTY_PROVIDER_EDITOR,
  providerToEditor,
  type ProviderEditorState,
} from "../providerProfile";

export function SettingsView() {
  const {
    providers,
    loadProviders,
    saveProfile,
    deleteProfile,
    setActiveProfile,
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
  const [editor, setEditor] = useState<ProviderEditorState | null>(null);
  const [editorKeyConfigured, setEditorKeyConfigured] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [savingProfile, setSavingProfile] = useState(false);
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

  const closeEditor = () => {
    setEditor(null);
    setEditorKeyConfigured(false);
    setProfileError(null);
  };

  const startEditing = (p: ProviderProfileInfo) => {
    setEditor(providerToEditor(p));
    setEditorKeyConfigured(p.has_key);
    setProfileError(null);
    setMessage(null);
  };

  const startNew = () => {
    setEditor({ ...EMPTY_PROVIDER_EDITOR });
    setEditorKeyConfigured(false);
    setProfileError(null);
    setMessage(null);
  };

  const saveProfileFromDialog = async (payload: ProviderSavePayload) => {
    if (savingProfile) return;
    setSavingProfile(true);
    setProfileError(null);
    try {
      await saveProfile(payload.profile, payload.apiKey, payload.clearKey);
      closeEditor();
      setMessage(`已保存 Provider Profile：${payload.profile.name}`);
    } catch (e) {
      setProfileError(String(e));
    } finally {
      setSavingProfile(false);
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
                          onClick={() => startEditing(p)}
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

          <footer className="card-actions">
            <button
              className="ghost"
              data-testid="profile-new"
              onClick={startNew}
            >
              新建 Profile
            </button>
          </footer>

          <ProviderEditorDialog
            target={editor}
            keyConfigured={editorKeyConfigured}
            saving={savingProfile}
            error={profileError}
            onSave={(payload) => void saveProfileFromDialog(payload)}
            onClose={closeEditor}
          />

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
