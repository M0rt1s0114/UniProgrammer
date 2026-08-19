import { call } from '@/services/ipc'

export interface SidecarDevice {
  id: string
  kind: string
  detail: string
}

export interface SidecarAdapterEntry {
  name: string
  devices: SidecarDevice[]
}

export interface SidecarError {
  0: string
  1: string
}

export function listSidecarAdapters(): Promise<SidecarAdapterEntry[]> {
  return call<SidecarAdapterEntry[]>('sidecar_adapters')
}

export function openSidecarSession(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_open', { adapter, device })
}

export function selectSidecarAdapter(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_select', { adapter, device })
}

export function unselectSidecarAdapter(): Promise<void> {
  return call<void>('sidecar_unselect')
}

export function closeSidecarSession(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_close', { adapter, device })
}

export function readSidecarId(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_read_id', { adapter, device })
}

export function eraseSidecarChip(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_erase', { adapter, device })
}

export function readSidecarChip(
  adapter: string,
  device: string,
  size: number,
  startAddr = 0,
): Promise<ArrayBuffer> {
  return call<ArrayBuffer>('sidecar_read', { adapter, device, size, startAddr })
}

export function writeSidecarChip(
  adapter: string,
  device: string,
  payload: Uint8Array,
  startAddr = 0,
): Promise<string> {
  return call<string>('sidecar_write', payload, {
    headers: {
      'x-adapter': adapter,
      'x-device': device,
      'x-start-addr': String(startAddr),
    },
  })
}

export function verifySidecarChip(
  adapter: string,
  device: string,
  payload: Uint8Array,
  startAddr = 0,
): Promise<string> {
  return call<string>('sidecar_verify', payload, {
    headers: {
      'x-adapter': adapter,
      'x-device': device,
      'x-start-addr': String(startAddr),
    },
  })
}

export function sidecarErrors(): Promise<SidecarError[]> {
  return call<SidecarError[]>('sidecar_errors')
}

export interface PluginInfo {
  name: string
  version: string
  kind: string
  enabled: boolean
  error: string | null
}

export interface BuiltinModule {
  name: string
  version: string
  description: string
}

export function listPlugins(): Promise<PluginInfo[]> {
  return call<PluginInfo[]>('plugin_list')
}

export function enablePlugin(name: string): Promise<string> {
  return call<string>('plugin_enable', { name })
}

export function disablePlugin(name: string): Promise<string> {
  return call<string>('plugin_disable', { name })
}

export function listBuiltinModules(): Promise<BuiltinModule[]> {
  return call<BuiltinModule[]>('plugin_builtin_modules')
}
