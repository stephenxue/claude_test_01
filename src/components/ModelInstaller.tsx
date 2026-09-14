import { useEffect, useState } from "react";
import { api, onEvent } from "../api";
import type { ModelPullProgressEvent, ModelsStatusDto } from "../types";
import { ProgressBar } from "./ProgressBar";

interface Props {
  enabled: boolean; // Ollama must be installed + running first
  onReady: (ready: boolean) => void;
}

export function ModelInstaller({ enabled, onReady }: Props) {
  const [status, setStatus] = useState<ModelsStatusDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ModelPullProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await onEvent<ModelPullProgressEvent>("model-pull-progress", setProgress);
    })();
    if (enabled) refresh();
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  useEffect(() => {
    onReady(!!status?.installed);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  async function refresh() {
    try {
      const s = await api.checkModelsStatus();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleInstall() {
    setBusy(true);
    setError(null);
    try {
      const s = await api.installModels();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  if (!enabled) {
    return (
      <div className="status-row">
        <span className="status-label">向量 / 大语言模型</span>
        <span className="status-text">等待 Ollama 就绪...</span>
      </div>
    );
  }

  const notInstalled = status && !status.installed;
  const ready = status && status.installed;

  return (
    <div className="status-row">
      <span className="status-label">向量 / 大语言模型</span>

      {!status && <span className="status-text">检测中...</span>}

      {notInstalled && (
        <>
          <span className="status-text status-red">向量模型和大语言模型没有安装</span>
          <button className="btn" disabled={busy} onClick={handleInstall}>
            安装模型
          </button>
        </>
      )}

      {ready && (
        <>
          <span className="status-text status-green">
            {status.recommendation.embedding.display_name} / {status.recommendation.chat.display_name}
          </span>
          <button className="btn btn-secondary" disabled>
            安装模型
          </button>
        </>
      )}

      {busy && (
        <ProgressBar
          percent={progress?.percent ?? 0}
          message={progress ? `${progress.model}：${progress.status}` : "准备下载..."}
        />
      )}
      {error && <span className="status-text status-red">{error}</span>}
    </div>
  );
}
