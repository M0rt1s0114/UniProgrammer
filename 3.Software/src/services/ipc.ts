import { invoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core'
import { listen, type Event, type UnlistenFn } from '@tauri-apps/api/event'

export type { UnlistenFn }

export function call<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  return invoke<T>(command, args, options)
}

export function onEvent<T>(event: string, handler: (event: Event<T>) => void): Promise<UnlistenFn> {
  return listen<T>(event, handler)
}

export function onProgress<T extends { done: number; total: number }>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return onEvent<T>(event, ({ payload }) => {
    handler(payload)
  })
}
