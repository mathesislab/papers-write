// The events of the backend for this web view: those sent to it (`emit_to`) and those sent to all (`emit`). Not the
// `listen` of @tauri-apps/api/event, whose listeners hear what is sent to any web view (their target is Any): the
// review pane heard the Undo and Forward of the page of its window, a page the menu commands of another window.

import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export const listen = <T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> =>
  getCurrentWebview().listen<T>(event, handler);
