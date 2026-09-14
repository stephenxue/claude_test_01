import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppConfig,
  ChatReplyDto,
  ChatTurn,
  DocumentListsDto,
  IndexSummary,
  ModelsStatusDto,
  OllamaStatusDto,
} from "./types";

export const api = {
  checkOllamaStatus: () => invoke<OllamaStatusDto>("check_ollama_status"),
  installOllama: () => invoke<OllamaStatusDto>("install_ollama"),
  startOllama: () => invoke<OllamaStatusDto>("start_ollama"),

  checkModelsStatus: () => invoke<ModelsStatusDto>("check_models_status"),
  installModels: () => invoke<ModelsStatusDto>("install_models"),

  getConfig: () => invoke<AppConfig>("get_config"),

  selectSourceFolder: (path: string) =>
    invoke<void>("select_source_folder", { path }),
  indexDocuments: () => invoke<IndexSummary>("index_documents"),
  listDocuments: () => invoke<DocumentListsDto>("list_documents"),
  deleteDocuments: (fileNames: string[]) =>
    invoke<void>("delete_documents", { fileNames }),
  restoreDocuments: (fileNames: string[]) =>
    invoke<void>("restore_documents", { fileNames }),

  chatSend: (message: string, history: ChatTurn[]) =>
    invoke<ChatReplyDto>("chat_send", { message, history }),

  writeTextFile: (path: string, content: string) =>
    invoke<void>("write_text_file", { path, content }),
  setAutoUpdateEnabled: (enabled: boolean) =>
    invoke<void>("set_auto_update_enabled", { enabled }),
  installUpdateFromFile: (path: string) =>
    invoke<void>("install_update_from_file", { path }),
};

export function onEvent<T>(name: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => handler(e.payload));
}

/**
 * Broadcasts a purely front-end event to any `onEvent` listener in the same
 * window - used e.g. so AboutDialog's "自动更新" toggle can tell UpdateDialog
 * to start/stop its hourly timer immediately, without needing an app
 * restart. This still round-trips through Tauri's IPC event bus (there's no
 * separate "local-only" event API), but works the same as a Rust-emitted
 * event from the listener's point of view.
 */
export function emitEvent<T>(name: string, payload: T): Promise<void> {
  return emit(name, payload);
}
