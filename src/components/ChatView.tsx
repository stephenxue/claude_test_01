import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { api, onEvent } from "../api";
import type { ChatTurn } from "../types";
import "./ChatView.css";

interface DisplayTurn extends ChatTurn {
  /** Only set on assistant turns that were grounded in the knowledge base. */
  sources?: string[];
}

function buildMarkdownExport(messages: DisplayTurn[]): string {
  const lines: string[] = ["# 对话记录", "", `导出时间：${new Date().toLocaleString()}`, ""];
  for (const m of messages) {
    lines.push(m.role === "user" ? "**你：**" : "**助手：**", "", m.content, "");
    if (m.role === "assistant" && m.sources && m.sources.length > 0) {
      lines.push(`> 引用来源：${m.sources.join("、")}`, "");
    }
  }
  return lines.join("\n");
}

/**
 * Saves `content` via a native "save as" dialog instead of the browser
 * `<a download>` blob trick the code used before - Tauri's macOS webview
 * (WKWebView) does not reliably fire that kind of download, so clicking
 * "下载对话" used to silently do nothing. Returns false if the user
 * cancelled the dialog (not an error - just nothing to report).
 */
async function downloadMarkdown(content: string): Promise<boolean> {
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  const path = await save({
    defaultPath: `对话记录-${stamp}.md`,
    filters: [{ name: "Markdown", extensions: ["md"] }],
  });
  if (!path) return false;
  await api.writeTextFile(path, content);
  return true;
}

export function ChatView() {
  const [messages, setMessages] = useState<DisplayTurn[]>([]);
  const [input, setInput] = useState("");
  const [pending, setPending] = useState(false); // waiting for the first token
  const [streamingText, setStreamingText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await onEvent<string>("chat-token", (token) => {
        setPending(false);
        setStreamingText((prev) => prev + token);
      });
    })();
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages, streamingText, pending]);

  async function handleSend() {
    const text = input.trim();
    if (!text || pending) return;
    setInput("");
    setError(null);

    const history = messages;
    const nextUserTurn: DisplayTurn = { role: "user", content: text };
    setMessages((prev) => [...prev, nextUserTurn]);
    setPending(true);
    setStreamingText("");

    try {
      const { reply, sources } = await api.chatSend(text, history);
      setMessages((prev) => [...prev, { role: "assistant", content: reply, sources }]);
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
      setStreamingText("");
    }
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  async function handleDownload() {
    if (messages.length === 0) return;
    try {
      await downloadMarkdown(buildMarkdownExport(messages));
    } catch (e) {
      setError(String(e));
    }
  }

  // Only the pre-first-token wait gets the italic/gray "thinking" treatment.
  // The moment real tokens start arriving we render them in the normal
  // assistant bubble style, so the text reads as continuously "typing in"
  // rather than popping from a gray placeholder into a different-looking
  // bubble once the whole reply is done.
  const showThinkingHint = pending && streamingText.length === 0;

  return (
    <div className="chat-view">
      <div className="chat-header">
        <h3>对话</h3>
        <button className="btn btn-secondary" onClick={handleDownload} disabled={messages.length === 0}>
          下载对话
        </button>
      </div>
      <div className="chat-messages" ref={scrollRef}>
        {messages.length === 0 && !pending && (
          <div className="chat-empty-hint">向知识库提问，例如："帮我总结一下最新的会议纪要"</div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={`chat-turn ${m.role}`}>
            <div className={`chat-bubble ${m.role}`}>{m.content}</div>
            {m.role === "assistant" && m.sources && m.sources.length > 0 && (
              <div className="chat-sources">引用来源：{m.sources.join("、")}</div>
            )}
          </div>
        ))}
        {pending && (
          <div className="chat-turn assistant">
            <div className={`chat-bubble assistant ${showThinkingHint ? "thinking" : ""}`}>
              {showThinkingHint ? "正在检索知识库并生成回答，请稍候..." : streamingText}
              {!showThinkingHint && <span className="chat-cursor">▌</span>}
            </div>
          </div>
        )}
        {error && <div className="chat-bubble assistant status-red">出错了：{error}</div>}
      </div>
      <div className="chat-input-bar">
        <textarea
          placeholder="输入问题，按 Enter 发送，Shift+Enter 换行"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button className="btn" onClick={handleSend} disabled={pending || !input.trim()}>
          发送
        </button>
      </div>
    </div>
  );
}
