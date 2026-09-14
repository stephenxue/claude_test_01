import { useEffect, useState } from "react";
import { api, onEvent } from "../api";
import type { InstallProgressEvent, OllamaStatusDto } from "../types";
import { ProgressBar } from "./ProgressBar";

interface Props {
  onReady: (ready: boolean) => void;
}

export function OllamaStatus({ onReady }: Props) {
  const [status, setStatus] = useState<OllamaStatusDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<InstallProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      const u = await onEvent<InstallProgressEvent>("ollama-install-progress", setProgress);
      unlisten = u;
    })();
    refresh();
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    onReady(!!status?.installed && !!status?.running);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  async function refresh() {
    try {
      const s = await api.checkOllamaStatus();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleInstall() {
    setBusy(true);
    setError(null);
    setProgress({ stage: "detecting", percent: 0, message: "准备安装..." });
    try {
      const s = await api.installOllama();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function handleStart() {
    setBusy(true);
    setError(null);
    try {
      const s = await api.startOllama();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const notInstalled = status && !status.installed;
  const installedNotRunning = status && status.installed && !status.running;
  const ready = status && status.installed && status.running;

  return (
    <div className="status-row">
      <span className="status-label">Ollama</span>

      {!status && <span className="status-text">检测中...</span>}

      {notInstalled && (
        <>
          <span className="status-text status-red">Ollama 没有安装</span>
          <button className="btn" disabled={busy} onClick={handleInstall}>
            安装 Ollama
          </button>
        </>
      )}

      {installedNotRunning && (
        <>
          <span className="status-text status-red">Ollama 没有运行</span>
          <button className="btn" disabled={busy} onClick={handleStart}>
            运行 Ollama
          </button>
        </>
      )}

      {ready && (
        <>
          <span className="status-text status-green">
            Ollama 已运行{status?.version ? `（${status.version}）` : ""}
          </span>
          <button className="btn btn-secondary" disabled>
            已就绪
          </button>
        </>
      )}

      {busy && progress && <ProgressBar percent={progress.percent} message={progress.message} />}
      {error && <span className="status-text status-red">{error}</span>}
    </div>
  );
}
