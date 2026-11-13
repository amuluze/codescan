/** 输出复制按钮：优先 Clipboard API，回退 textarea + execCommand（WKWebView 兼容）。 */
import { useEffect, useRef, useState } from "react";

async function writeClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  if (!ok) throw new Error("复制失败");
}

type Props = {
  text: string;
  /** 无障碍名称，也用于测试定位（如「复制分诊建议」）。 */
  label: string;
};

export function CopyButton({ text, label }: Props) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const copy = async () => {
    try {
      await writeClipboard(text);
      setState("copied");
    } catch {
      setState("failed");
    }
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState("idle"), 1600);
  };

  return (
    <button
      type="button"
      className="ghost copy-button"
      onClick={() => void copy()}
      disabled={text.trim() === ""}
      aria-label={label}
      title={state === "failed" ? "复制失败" : "复制"}
    >
      {state === "failed" ? "复制失败" : state === "copied" ? "已复制" : "复制"}
    </button>
  );
}
