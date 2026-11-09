/** 设置页：LLM Provider（复刻 axiom 形态）与 API Key（只写）。 */
import { useEffect, useState } from "react";
import { useLlmStore } from "../stores/llmStore";

export function SettingsView() {
  const { provider, loadProvider, saveProvider, saveApiKey, clearApiKey } = useLlmStore();
  const [kind, setKind] = useState<string>("openai");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    loadProvider()
      .then(() => {
        const p = useLlmStore.getState().provider;
        if (p) {
          setKind(p.kind);
          setBaseUrl(p.base_url);
          setModel(p.model);
        }
      })
      .catch((e) => setMessage(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async () => {
    setMessage(null);
    try {
      await saveProvider(kind, baseUrl, model);
      setMessage("Provider 配置已保存");
    } catch (e) {
      setMessage(String(e));
    }
  };

  const saveKey = async () => {
    setMessage(null);
    try {
      await saveApiKey(apiKey);
      setApiKey("");
      setMessage("API Key 已保存（仅存本地 SQLite secrets 表）");
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <section className="settings-view" aria-label="设置">
      <h3>LLM Provider（兼容 OpenAI / Anthropic）</h3>
      <div className="form-grid">
        <label>
          Provider
          <select aria-label="Provider" value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="openai">OpenAI-compatible</option>
            <option value="anthropic">Anthropic-compatible</option>
          </select>
        </label>
        <label>
          Base URL
          <input
            aria-label="Base URL"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.openai.com/v1"
          />
        </label>
        <label>
          Model
          <input
            aria-label="Model"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="模型名，必填"
          />
        </label>
      </div>
      <div className="row">
        <button onClick={() => void save()}>保存 Provider 配置</button>
      </div>

      <h3>API Key</h3>
      <p className="muted" data-testid="key-status">
        {provider?.has_key ? "已配置（明文不可读取）" : "未配置"}
      </p>
      <div className="row">
        <input
          aria-label="API Key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
        />
        <button onClick={() => void saveKey()} disabled={!apiKey.trim()}>
          保存 Key
        </button>
        <button onClick={() => void clearApiKey()} disabled={!provider?.has_key}>
          清除 Key
        </button>
      </div>

      {message && <p role="status">{message}</p>}
      <p className="muted">
        密钥仅存于 ~/.codescan/codescan.db 的 secrets 表（Rust 独占）；命令面不提供明文回读。
      </p>
    </section>
  );
}
