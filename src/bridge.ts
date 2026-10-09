import type { Status } from './types';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { preview } from './preview';
export const native = '__TAURI_INTERNALS__' in window;
export const tray = native ? getCurrentWindow().label === 'tray' : new URLSearchParams(location.search).get('view') === 'tray';
export const floating = tray || (native ? getCurrentWindow().label === 'recommendation' : new URLSearchParams(location.search).get('view') === 'card');
export async function command(name: string, args: Record<string, unknown> = {}): Promise<Status> {
  if (native) return invoke<Status>(name, args);
  return preview(name, args);
}
export async function watch(callback: (status: Status) => void): Promise<() => void> {
  if (!native) return () => {};
  return listen<Status>('habitos:status',event=>callback(event.payload));
}
