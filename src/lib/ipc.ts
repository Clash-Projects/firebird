import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function ipcInvoke<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T | null> {
  if (!isTauriRuntime()) return null;
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    console.warn(`[ipc] ${command} failed:`, err);
    return null;
  }
}

export async function ipcListen<T>(
  event: string,
  handler: (payload: T) => void
): Promise<UnlistenFn | null> {
  if (!isTauriRuntime()) return null;
  try {
    return await listen<T>(event, (e) => handler(e.payload));
  } catch (err) {
    console.warn(`[ipc] listen(${event}) failed:`, err);
    return null;
  }
}
