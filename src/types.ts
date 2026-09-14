export interface OllamaStatusDto {
  installed: boolean;
  running: boolean;
  path: string | null;
  version: string | null;
}

export interface ModelSpec {
  tag: string;
  display_name: string;
  license: string;
  approx_download_gb: number;
}

export interface ModelChoice {
  embedding: ModelSpec;
  chat: ModelSpec;
  embedding_dim: number;
  tier: string;
  reason: string;
}

export interface ModelsStatusDto {
  installed: boolean;
  embedding_model: string | null;
  chat_model: string | null;
  recommendation: ModelChoice;
}

export interface AppConfig {
  ollama_installed: boolean;
  ollama_path: string | null;
  ollama_version: string | null;
  embedding_model_tag: string | null;
  chat_model_tag: string | null;
  embedding_dim: number | null;
  models_installed: boolean;
  source_folder: string | null;
  last_indexed_at: string | null;
  auto_update_enabled: boolean;
}

export interface InstallProgressEvent {
  stage: "detecting" | "downloading" | "extracting" | "done" | "error";
  percent: number;
  message: string;
}

export interface ModelPullProgressEvent {
  model: string;
  status: string;
  percent: number;
}

export interface IndexProgressEvent {
  stage: "ingesting" | "done";
  file_name: string;
  current: number;
  total: number;
  /** Best-effort ETA in seconds, based on the pace so far. `null` until the
   * first file in this run has finished. */
  eta_seconds: number | null;
}

export interface IndexSummary {
  ingested_files: number;
  chunks_added: number;
}

export interface DocumentEntry {
  file_name: string;
  size_bytes: number;
}

/** The two-column "已索引 / 已删除" document lists. */
export interface DocumentListsDto {
  indexed: DocumentEntry[];
  deleted: DocumentEntry[];
}

export interface DocumentActionProgressEvent {
  action: "deleting" | "restoring";
  file_name: string;
  current: number;
  total: number;
}

export interface ChatTurn {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface ChatReplyDto {
  reply: string;
  /** Distinct source file names the answer drew from, shown as citations. */
  sources: string[];
}
