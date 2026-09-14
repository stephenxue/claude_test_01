import { useEffect, useState } from "react";
import "./App.css";
import { AboutDialog } from "./components/AboutDialog";
import { ChatView } from "./components/ChatView";
import { DocumentManager } from "./components/DocumentManager";
import { FolderPicker } from "./components/FolderPicker";
import { IndexStartPanel } from "./components/IndexStartPanel";
import { ModelInstaller } from "./components/ModelInstaller";
import { OllamaStatus } from "./components/OllamaStatus";
import { UpdateDialog } from "./components/UpdateDialog";
import { api, onEvent } from "./api";
import type { IndexProgressEvent, IndexSummary } from "./types";

export default function App() {
  const [ollamaReady, setOllamaReady] = useState(false);
  const [modelsReady, setModelsReady] = useState(false);
  const [sourceFolder, setSourceFolder] = useState<string | null>(null);

  const [indexing, setIndexing] = useState(false);
  const [indexProgress, setIndexProgress] = useState<IndexProgressEvent | null>(null);
  const [lastSummary, setLastSummary] = useState<IndexSummary | null>(null);
  const [docRefreshKey, setDocRefreshKey] = useState(0);

  // Whether the currently-selected folder has been scanned at least once.
  // Vectorization is a deliberate, explicit action (see IndexStartPanel) -
  // picking a folder only sets it up, it never auto-starts indexing.
  const [hasIndexedOnce, setHasIndexedOnce] = useState(false);

  // Load persisted config on startup (so a second launch shows the saved
  // source folder immediately, per spec item 6). If this folder was already
  // indexed in a previous session, skip straight to the chat/document view.
  useEffect(() => {
    api.getConfig().then((cfg) => {
      if (cfg.source_folder) setSourceFolder(cfg.source_folder);
      if (cfg.source_folder && cfg.last_indexed_at) setHasIndexedOnce(true);
    });
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await onEvent<IndexProgressEvent>("index-progress", setIndexProgress);
    })();
    return () => unlisten?.();
  }, []);

  async function runIndex() {
    setIndexing(true);
    setIndexProgress(null);
    try {
      const summary = await api.indexDocuments();
      setLastSummary(summary);
      setHasIndexedOnce(true);
    } catch (e) {
      console.error(e);
    } finally {
      setIndexing(false);
      setDocRefreshKey((k) => k + 1);
    }
  }

  const canPickFolder = ollamaReady && modelsReady;

  return (
    <div className="app">
      <div className="setup-panel">
        <OllamaStatus onReady={setOllamaReady} />
        <ModelInstaller enabled={ollamaReady} onReady={setModelsReady} />
        <FolderPicker
          enabled={canPickFolder}
          currentFolder={sourceFolder}
          onSelected={(path) => {
            setSourceFolder(path);
            setHasIndexedOnce(false);
            setLastSummary(null);
          }}
        />
      </div>

      <div className="main-body">
        {!canPickFolder || !sourceFolder ? (
          <div className="locked-notice">
            请先完成上方 Ollama、模型安装，并选择文档文件夹。
          </div>
        ) : !hasIndexedOnce ? (
          <IndexStartPanel
            folder={sourceFolder}
            indexing={indexing}
            indexProgress={indexProgress}
            onStart={runIndex}
          />
        ) : (
          <>
            <DocumentManager
              refreshKey={docRefreshKey}
              indexing={indexing}
              indexProgress={indexProgress}
              lastSummary={lastSummary}
              onReindex={runIndex}
            />
            <ChatView />
          </>
        )}
      </div>

      <AboutDialog />
      <UpdateDialog />
    </div>
  );
}
