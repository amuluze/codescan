/** 设置页：LLM Provider（兼容 OpenAI / Anthropic）与 API Key（只写）。 */
import { useEffect, useState } from "react";
import { useLlmStore } from "../stores/llmStore";

export function SettingsView() {
  const {
    provider,
    loadProvider,
    saveProvider,
    saveApiKey,
    clearApiKey,
    testing,
    testResult,
    testError,
    testConnection,
  } = useLlmStore();
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
              <p className="card-sub">兼容 OpenAI 与 Anthropic 协议的本地直连；无内置遥测，请求只发往你填写的 Base URL。</p>
            </div>
          </header>
          <div className="form-grid">
            <label className="field">
              <span className="field-label">Provider</span>
              <select aria-label="Provider" value={kind} onChange={(e) => setKind(e.target.value)}>
                <option value="openai">OpenAI-compatible</option>
                <option value="anthropic">Anthropic-compatible</option>
              </select>
            </label>
            <label className="field">
              <span className="field-label">Base URL</span>
              <input
                aria-label="Base URL"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                placeholder="https://api.openai.com/v1"
              />
            </label>
            <label className="field field-wide">
              <span className="field-label">Model</span>
              <input
                aria-label="Model"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="模型名，必填（例如 gpt-4o-mini / claude-sonnet-4-5）"
              />
            </label>
          </div>
          <div className="key-block">
            <div className="key-head">
              <span className="field-label">API Key</span>
              <span
                className={`status-chip ${provider?.has_key ? "ok" : "muted-chip"}`}
                data-testid="key-status"
              >
                {provider?.has_key ? "已配置" : "未配置"}
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
              <button className="primary" onClick={() => void saveKey()} disabled={!apiKey.trim()}>
                保存 Key
              </button>
              <button className="ghost" onClick={() => void clearApiKey()} disabled={!provider?.has_key}>
                清除 Key
              </button>
            </div>
            <p className="key-note">仅写入 Rust 独占的 SQLite secrets 表；命令面不提供明文回读。</p>
          </div>
          <footer className="card-actions">
            <button className="primary" onClick={() => void save()}>
              保存 Provider 配置
            </button>
            <button className="ghost" onClick={() => void testConnection()} disabled={testing}>
              {testing ? "测试中…" : "测试连接"}
            </button>
          </footer>
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
