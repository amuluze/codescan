/** 设置页：LLM Provider Profile（多 Profile · 兼容 OpenAI / Anthropic）、API Key（只写）与应用更新。 */
import { useEffect } from "react";
import { useUpdateStore } from "../stores/updateStore";
import {
  ProviderProfilesCard,
  ReviewSkillCard,
  SafetyNotesCard,
  UpdateCard,
  useProviderProfiles,
  useReviewSkill,
} from "./SettingsView.parts";

export function SettingsView() {
  const profiles = useProviderProfiles();
  const skill = useReviewSkill();
  const {
    available: updateInfo,
    checking: updateChecking,
    installing: updateInstalling,
    message: updateMessage,
    error: updateError,
    check: checkUpdate,
    install: installUpdate,
  } = useUpdateStore();

  useEffect(() => {
    profiles.loadProviders().catch((e) => profiles.setMessage(String(e)));
    skill.loadInfo();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

      {profiles.message && (
        <p role="status" className="notice">
          {profiles.message}
        </p>
      )}

      <div className="settings-stack">
        <ProviderProfilesCard model={profiles} />
        <ReviewSkillCard model={skill} />
        <UpdateCard
          available={updateInfo}
          checking={updateChecking}
          installing={updateInstalling}
          message={updateMessage}
          error={updateError}
          onCheck={() => void checkUpdate()}
          onInstall={() => void installUpdate()}
        />
        <SafetyNotesCard />
      </div>
    </section>
  );
}
