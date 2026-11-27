/**
 * 设置页的构建块：页面态 hook（Provider Profile 操作、内置审查 Skill）与各卡片展示组件。
 * 展示组件均由 props 驱动、无自有状态；页面骨架与挂载加载见 SettingsView.tsx。
 */
import { useState, type Dispatch, type SetStateAction } from "react";
import { useLlmStore } from "../stores/llmStore";
import {
  api,
  type ProviderProfileInfo,
  type ProviderTestResult,
  type ProvidersInfo,
  type ReviewSkillFileInfo,
  type ReviewSkillInfo,
  type UpdateInfo,
} from "../platform";
import {
  ProviderEditorDialog,
  type ProviderSavePayload,
} from "../components/ProviderEditorDialog";
import { SkillEditorDialog } from "../components/SkillEditorDialog";
import {
  EMPTY_PROVIDER_EDITOR,
  providerToEditor,
  type ProviderEditorState,
} from "../providerProfile";

/** 行内操作只依赖 Profile 的 id 与名称（提示文案用）。 */
type ProfileRef = { id: string; name: string };

/** Provider Profile 卡的页面态：编辑器、保存 / 设为默认 / 删除 / 测试与页面级提示。 */
type ProviderProfilesModel = {
  providers: ProvidersInfo | null;
  testing: boolean;
  testResult: ProviderTestResult | null;
  testError: string | null;
  /** 打开中的编辑器目标；null = 弹窗关闭。 */
  editor: ProviderEditorState | null;
  editorKeyConfigured: boolean;
  /** 页面级提示（保存 / 切换默认 / 删除的结果与加载失败）。 */
  message: string | null;
  setMessage: Dispatch<SetStateAction<string | null>>;
  /** 弹窗内的保存错误。 */
  profileError: string | null;
  savingProfile: boolean;
  /** 进行中的设为默认 / 删除对象：用于禁用同组其它操作。 */
  activatingId: string | null;
  deletingId: string | null;
  /** 最近一次「测试」的 Profile 名称：用于行内「测试中…」文案。 */
  testedName: string | null;
  /** 待确认删除的 Profile id（内联二次确认）。 */
  confirmDeleteId: string | null;
  setConfirmDeleteId: Dispatch<SetStateAction<string | null>>;
  loadProviders: () => Promise<void>;
  closeEditor: () => void;
  startEditing: (profile: ProviderProfileInfo) => void;
  startNew: () => void;
  saveProfileFromDialog: (payload: ProviderSavePayload) => Promise<void>;
  activate: (profile: ProfileRef) => Promise<void>;
  removeProfile: (profile: ProfileRef) => Promise<void>;
  testProfile: (profile: ProfileRef) => Promise<void>;
};

/** 内置审查 Skill 卡的页面态：文件清单、覆写编辑与保存 / 恢复。 */
type ReviewSkillModel = {
  skillInfo: ReviewSkillInfo | null;
  skillError: string | null;
  skillMessage: string | null;
  /** 打开中的技能文件编辑器；null = 弹窗关闭。 */
  editingSkill: ReviewSkillFileInfo | null;
  skillSaving: boolean;
  /** 已本地覆写的文件数（卡片头部徽标）。 */
  customizedSkills: number;
  /** 拉取文件清单；失败写入 skillError。 */
  loadInfo: () => void;
  startEditing: (file: ReviewSkillFileInfo) => void;
  closeEditor: () => void;
  saveSkill: (content: string) => Promise<void>;
  resetSkill: (name: string) => Promise<void>;
};

export function useProviderProfiles(): ProviderProfilesModel {
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
  const [editor, setEditor] = useState<ProviderEditorState | null>(null);
  const [editorKeyConfigured, setEditorKeyConfigured] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [savingProfile, setSavingProfile] = useState(false);
  const [activatingId, setActivatingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [testedName, setTestedName] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

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

  const activate = async (p: ProfileRef) => {
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

  const removeProfile = async (p: ProfileRef) => {
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

  const testProfile = async (p: ProfileRef) => {
    setTestedName(p.name);
    await testConnection(p.id);
  };

  return {
    providers,
    testing,
    testResult,
    testError,
    editor,
    editorKeyConfigured,
    message,
    setMessage,
    profileError,
    savingProfile,
    activatingId,
    deletingId,
    testedName,
    confirmDeleteId,
    setConfirmDeleteId,
    loadProviders,
    closeEditor,
    startEditing,
    startNew,
    saveProfileFromDialog,
    activate,
    removeProfile,
    testProfile,
  };
}

export function useReviewSkill(): ReviewSkillModel {
  const [skillInfo, setSkillInfo] = useState<ReviewSkillInfo | null>(null);
  const [skillError, setSkillError] = useState<string | null>(null);
  const [skillMessage, setSkillMessage] = useState<string | null>(null);
  const [editingSkill, setEditingSkill] = useState<ReviewSkillFileInfo | null>(null);
  const [skillSaving, setSkillSaving] = useState(false);

  const loadInfo = () => {
    void api
      .llmReviewSkillInfo()
      .then(setSkillInfo)
      .catch((e) => setSkillError(String(e)));
  };

  const startEditing = (file: ReviewSkillFileInfo) => {
    setSkillError(null);
    setEditingSkill(file);
  };

  const closeEditor = () => {
    setEditingSkill(null);
    setSkillError(null);
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

  return {
    skillInfo,
    skillError,
    skillMessage,
    editingSkill,
    skillSaving,
    customizedSkills: skillInfo?.files.filter((file) => file.customized).length ?? 0,
    loadInfo,
    startEditing,
    closeEditor,
    saveSkill,
    resetSkill,
  };
}

/** 模型 Provider 卡：Profile 表格 + 编辑弹窗 + 连接测试结果。 */
export function ProviderProfilesCard({ model }: { model: ProviderProfilesModel }) {
  const {
    providers,
    editor,
    editorKeyConfigured,
    savingProfile,
    profileError,
    testResult,
    testError,
    testedName,
    startNew,
    closeEditor,
    saveProfileFromDialog,
  } = model;
  return (
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
        <ProviderTable providers={providers} model={model} />
      )}
      {providers && providers.profiles.length === 0 && (
        <p className="card-sub">尚无 Provider Profile；新建一个以启用 LLM 辅助能力。</p>
      )}

      <footer className="card-actions">
        <button className="ghost" data-testid="profile-new" onClick={startNew}>
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
  );
}

function ProviderTable({
  providers,
  model,
}: {
  /** 调用方已保证至少有一个 Profile。 */
  providers: ProvidersInfo;
  model: ProviderProfilesModel;
}) {
  return (
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
          <ProviderRow key={p.id} profile={p} providers={providers} model={model} />
        ))}
      </tbody>
    </table>
  );
}

function ProviderRow({
  profile: p,
  providers,
  model,
}: {
  profile: ProviderProfileInfo;
  providers: ProvidersInfo;
  model: ProviderProfilesModel;
}) {
  const {
    editor,
    activatingId,
    deletingId,
    savingProfile,
    testing,
    testedName,
    confirmDeleteId,
    setConfirmDeleteId,
    startEditing,
    activate,
    testProfile,
    removeProfile,
  } = model;
  return (
    <tr key={p.id} className={editor?.id === p.id ? "editing" : undefined}>
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
  );
}

/** LLM 审查内置 Skill 卡：文件表（覆写状态）+ 编辑弹窗。 */
export function ReviewSkillCard({ model }: { model: ReviewSkillModel }) {
  const {
    skillInfo,
    skillError,
    skillMessage,
    editingSkill,
    skillSaving,
    customizedSkills,
    startEditing,
    closeEditor,
    saveSkill,
    resetSkill,
  } = model;
  return (
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
                  <span className={`status-chip ${file.customized ? "ok" : "muted-chip"}`}>
                    {file.customized ? "已覆写" : "内置"}
                  </span>
                </td>
                <td>
                  <div className="table-actions">
                    <button
                      className="ghost btn-sm"
                      aria-label={`编辑 ${file.name}`}
                      onClick={() => startEditing(file)}
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
        onClose={closeEditor}
      />
    </section>
  );
}

type UpdateCardProps = {
  /** 已发现的新版本；null = 当前无可用更新。 */
  available: UpdateInfo | null;
  checking: boolean;
  installing: boolean;
  message: string | null;
  error: string | null;
  onCheck: () => void;
  onInstall: () => void;
};

/** 应用更新卡：检查更新 + 下载安装，以及状态 / 错误提示。 */
export function UpdateCard({
  available: updateInfo,
  checking: updateChecking,
  installing: updateInstalling,
  message: updateMessage,
  error: updateError,
  onCheck,
  onInstall,
}: UpdateCardProps) {
  return (
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
          onClick={onCheck}
          disabled={updateChecking || updateInstalling}
        >
          {updateChecking ? "检查中…" : "检查更新"}
        </button>
        {updateInfo && (
          <button
            className="primary"
            data-testid="update-install"
            onClick={onInstall}
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
  );
}

/** 安全边界说明卡（静态文案）。 */
export function SafetyNotesCard() {
  return (
    <section className="card card-note">
      <h3>安全边界</h3>
      <ul className="note-list">
        <li>LLM 输出仅作展示参考，永不进入门禁与基线；只有经校验并显式保存的规则才影响后续扫描。</li>
        <li>密钥不出现在日志、错误消息、发现或任何导出物中。</li>
        <li>扫描本身只读工作区；历史、基线与规则写入 <code>~/.codescan/</code>。</li>
      </ul>
    </section>
  );
}
