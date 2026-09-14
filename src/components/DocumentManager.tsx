import { useEffect, useState } from "react";
import { api, onEvent } from "../api";
import type {
  DocumentActionProgressEvent,
  DocumentEntry,
  IndexProgressEvent,
  IndexSummary,
} from "../types";
import "./DocumentManager.css";

interface Props {
  refreshKey: number; // bump this to force a re-fetch after indexing completes
  indexing: boolean;
  indexProgress: IndexProgressEvent | null;
  lastSummary: IndexSummary | null;
  onReindex: () => void;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatEta(seconds: number | null | undefined): string {
  if (seconds == null) return "";
  if (seconds < 1) return "即将完成";
  if (seconds < 60) return ` · 预计剩余约 ${Math.ceil(seconds)} 秒`;
  return ` · 预计剩余约 ${Math.ceil(seconds / 60)} 分钟`;
}

function toggleSet(set: Set<string>, name: string): Set<string> {
  const next = new Set(set);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  return next;
}

/** Intersects a selection set with the current list, dropping names that
 * have moved to the other column (or disappeared). Returns the same
 * reference when nothing changed, so it's safe to call every render. */
function pruneSelection(selected: Set<string>, entries: DocumentEntry[]): Set<string> {
  const valid = new Set(entries.map((d) => d.file_name));
  const next = new Set<string>();
  for (const name of selected) {
    if (valid.has(name)) next.add(name);
  }
  return next.size === selected.size ? selected : next;
}

export function DocumentManager({ refreshKey, indexing, indexProgress, lastSummary, onReindex }: Props) {
  const [indexed, setIndexed] = useState<DocumentEntry[]>([]);
  const [deleted, setDeleted] = useState<DocumentEntry[]>([]);
  const [selectedIndexed, setSelectedIndexed] = useState<Set<string>>(new Set());
  const [selectedDeleted, setSelectedDeleted] = useState<Set<string>>(new Set());
  const [actionBusy, setActionBusy] = useState(false);
  const [actionProgress, setActionProgress] = useState<DocumentActionProgressEvent | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    if (indexing || actionBusy) return;
    api
      .listDocuments()
      .then((lists) => {
        setIndexed(lists.indexed);
        setDeleted(lists.deleted);
      })
      .catch(() => {
        setIndexed([]);
        setDeleted([]);
      });
  }, [refreshKey, indexing, actionBusy]);

  useEffect(() => {
    setSelectedIndexed((prev) => pruneSelection(prev, indexed));
  }, [indexed]);
  useEffect(() => {
    setSelectedDeleted((prev) => pruneSelection(prev, deleted));
  }, [deleted]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await onEvent<DocumentActionProgressEvent>(
        "document-action-progress",
        setActionProgress,
      );
    })();
    return () => unlisten?.();
  }, []);

  async function handleDelete() {
    const names = Array.from(selectedIndexed);
    if (names.length === 0 || actionBusy || indexing) return;
    setActionBusy(true);
    setActionProgress(null);
    setActionError(null);
    try {
      await api.deleteDocuments(names);
    } catch (e) {
      console.error(e);
      setActionError(String(e));
    } finally {
      setActionBusy(false);
      setActionProgress(null);
    }
  }

  async function handleRestore() {
    const names = Array.from(selectedDeleted);
    if (names.length === 0 || actionBusy || indexing) return;
    setActionBusy(true);
    setActionProgress(null);
    setActionError(null);
    try {
      await api.restoreDocuments(names);
    } catch (e) {
      console.error(e);
      setActionError(String(e));
    } finally {
      setActionBusy(false);
      setActionProgress(null);
    }
  }

  const indexingPercent =
    indexProgress && indexProgress.total > 0 ? (indexProgress.current / indexProgress.total) * 100 : 0;
  const actionPercent =
    actionProgress && actionProgress.total > 0 ? (actionProgress.current / actionProgress.total) * 100 : 0;

  return (
    <div className="doc-manager">
      <div className="doc-manager-header">
        <h3>文档管理</h3>
        <button className="btn btn-secondary" onClick={onReindex} disabled={indexing || actionBusy}>
          {indexing ? "正在扫描..." : "重新扫描文件夹"}
        </button>
      </div>

      {indexing && (
        <div className="doc-status-bar">
          <div className="doc-status-bar-fill" style={{ width: `${indexingPercent}%` }} />
          <span className="doc-status-bar-text">
            {indexProgress?.stage === "ingesting" && `正在向量化: ${indexProgress.file_name}`}
            {(!indexProgress || indexProgress.stage === "done") && "正在扫描文件夹..."}
            {indexProgress && indexProgress.total > 0
              ? ` (${indexProgress.current}/${indexProgress.total})`
              : ""}
            {indexProgress?.stage === "ingesting" ? formatEta(indexProgress.eta_seconds) : ""}
          </span>
        </div>
      )}

      {actionBusy && (
        <div className="doc-status-bar">
          <div className="doc-status-bar-fill" style={{ width: `${actionPercent}%` }} />
          <span className="doc-status-bar-text">
            {actionProgress?.action === "deleting" && `正在删除: ${actionProgress.file_name}`}
            {actionProgress?.action === "restoring" && `正在恢复: ${actionProgress.file_name}`}
            {!actionProgress && "处理中..."}
            {actionProgress && actionProgress.total > 0
              ? ` (${actionProgress.current}/${actionProgress.total})`
              : ""}
          </span>
        </div>
      )}

      {!indexing && !actionBusy && lastSummary && (
        <div className="doc-status-bar doc-status-bar-static">
          <span className="doc-status-bar-text">
            上次扫描：新增 {lastSummary.ingested_files} 个文件（{lastSummary.chunks_added} 个片段）
          </span>
        </div>
      )}

      {actionError && (
        <div className="doc-status-bar doc-status-bar-error">
          <span className="doc-status-bar-text">操作失败：{actionError}</span>
        </div>
      )}

      <div className="doc-columns">
        <div className="doc-column">
          <div className="doc-column-header">
            <span>已索引 ({indexed.length})</span>
            <button
              className="btn btn-danger-sm"
              onClick={handleDelete}
              disabled={selectedIndexed.size === 0 || actionBusy || indexing}
            >
              删除
            </button>
          </div>
          <ul className="doc-list">
            {indexed.map((d) => (
              <li key={d.file_name} className="doc-list-item">
                <label className="doc-checkbox-label">
                  <input
                    type="checkbox"
                    checked={selectedIndexed.has(d.file_name)}
                    onChange={() => setSelectedIndexed((prev) => toggleSet(prev, d.file_name))}
                    disabled={actionBusy}
                  />
                  <span className="doc-name" title={d.file_name}>
                    {d.file_name}
                  </span>
                </label>
                <span className="doc-size">{formatSize(d.size_bytes)}</span>
              </li>
            ))}
            {indexed.length === 0 && (
              <li className="doc-empty">还没有已索引的文档，把文本文件放进所选文件夹即可自动索引。</li>
            )}
          </ul>
        </div>

        <div className="doc-column">
          <div className="doc-column-header">
            <span>已删除 ({deleted.length})</span>
            <button
              className="btn btn-secondary-sm"
              onClick={handleRestore}
              disabled={selectedDeleted.size === 0 || actionBusy || indexing}
            >
              恢复
            </button>
          </div>
          <ul className="doc-list">
            {deleted.map((d) => (
              <li key={d.file_name} className="doc-list-item">
                <label className="doc-checkbox-label">
                  <input
                    type="checkbox"
                    checked={selectedDeleted.has(d.file_name)}
                    onChange={() => setSelectedDeleted((prev) => toggleSet(prev, d.file_name))}
                    disabled={actionBusy}
                  />
                  <span className="doc-name" title={d.file_name}>
                    {d.file_name}
                  </span>
                </label>
                <span className="doc-size">{formatSize(d.size_bytes)}</span>
              </li>
            ))}
            {deleted.length === 0 && <li className="doc-empty">暂无已删除的文档。</li>}
          </ul>
        </div>
      </div>
    </div>
  );
}
