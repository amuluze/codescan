/** Provider Profile 编辑器的共享类型与校验/端点预览（弹窗与设置页共用）。 */
import type { ProviderProfileInfo } from "./platform";

export type ProviderEditorState = {
  id: string | null;
  name: string;
  kind: string;
  baseUrl: string;
  model: string;
};

export const EMPTY_PROVIDER_EDITOR: ProviderEditorState = {
  id: null,
  name: "",
  kind: "openai",
  baseUrl: "",
  model: "",
};

export const providerToEditor = (
  profile: ProviderProfileInfo,
): ProviderEditorState => ({
  id: profile.id,
  name: profile.name,
  kind: profile.kind,
  baseUrl: profile.base_url,
  model: profile.model,
});

/** Base URL 必须是可解析的 http(s) 地址；与后端 ProviderConfig::validate 同口径。 */
export const isHttpUrl = (value: string): boolean => {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
};

export type ProfileFieldErrors = {
  name?: string;
  baseUrl?: string;
  model?: string;
};

/** 客户端校验：与后端一致的三项必填 + Base URL 协议。 */
export const profileFieldErrors = (
  editor: ProviderEditorState,
): ProfileFieldErrors => {
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
export const endpointPreview = (editor: ProviderEditorState): string => {
  const base = editor.baseUrl.trim().replace(/\/+$/, "");
  if (!base || !isHttpUrl(base)) return "—";
  return editor.kind === "anthropic"
    ? `${base}/v1/messages`
    : `${base}/chat/completions`;
};

/** 常见重复路径：Base URL 已包含协议追加的路径时提示。 */
export const endpointWarning = (
  editor: ProviderEditorState,
): string | null => {
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
