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

export function closeSidecarSession(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_close', { adapter, device })
}

export function readSidecarId(adapter: string, device: string): Promise<string> {
  return call<string>('sidecar_read_id', { adapter, device })
}

export function sidecarErrors(): Promise<SidecarError[]> {
  return call<SidecarError[]>('sidecar_errors')
}
