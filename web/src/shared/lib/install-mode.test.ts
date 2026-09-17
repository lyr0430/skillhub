import { describe, expect, it, vi } from 'vitest'
import {
  DEFAULT_INSTALL_MODE,
  INSTALL_MODE_STORAGE_KEY,
  isInstallMode,
  readStoredInstallMode,
  resolveInstallMode,
  saveInstallMode,
  type InstallMode,
} from './install-mode'

/** In-memory stand-in for `localStorage`. */
function createStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, value)
    },
    raw: map,
  }
}

describe('isInstallMode', () => {
  it('accepts the two known modes', () => {
    expect(isInstallMode('shared')).toBe(true)
    expect(isInstallMode('copy')).toBe(true)
  })

  it('rejects anything else', () => {
    expect(isInstallMode('symlink')).toBe(false)
    expect(isInstallMode('')).toBe(false)
    expect(isInstallMode(null)).toBe(false)
    expect(isInstallMode(undefined)).toBe(false)
  })
})

describe('readStoredInstallMode', () => {
  it('returns the stored mode', () => {
    const storage = createStorage({ [INSTALL_MODE_STORAGE_KEY]: 'copy' })
    expect(readStoredInstallMode(storage)).toBe('copy')
  })

  it('returns null when nothing is stored, so the caller applies the default', () => {
    expect(readStoredInstallMode(createStorage())).toBeNull()
  })

  it('returns null for an unrecognised value instead of throwing', () => {
    const storage = createStorage({ [INSTALL_MODE_STORAGE_KEY]: 'hardlink' })
    expect(readStoredInstallMode(storage)).toBeNull()
  })

  it('returns null when storage is unavailable', () => {
    expect(readStoredInstallMode(undefined)).toBeNull()
  })

  it('returns null when reading throws', () => {
    const storage = {
      getItem: vi.fn(() => {
        throw new Error('blocked')
      }),
      setItem: vi.fn(),
    }
    expect(readStoredInstallMode(storage)).toBeNull()
  })
})

describe('saveInstallMode', () => {
  it('round-trips through storage', () => {
    const storage = createStorage()
    saveInstallMode('copy', storage)
    expect(readStoredInstallMode(storage)).toBe('copy')
  })

  it('swallows write failures', () => {
    const storage = {
      getItem: vi.fn(() => null),
      setItem: vi.fn(() => {
        throw new Error('quota')
      }),
    }
    expect(() => saveInstallMode('shared', storage)).not.toThrow()
  })

  it('does nothing when storage is unavailable', () => {
    expect(() => saveInstallMode('copy', undefined)).not.toThrow()
  })
})

describe('resolveInstallMode', () => {
  it('returns the stored mode when set', () => {
    const storage = createStorage({ [INSTALL_MODE_STORAGE_KEY]: 'copy' })
    expect(resolveInstallMode(storage)).toBe('copy')
  })

  it('falls back to the default when unset', () => {
    expect(resolveInstallMode(createStorage())).toBe('shared')
  })

  it('falls back to the default when the stored value is unrecognised', () => {
    const storage = createStorage({ [INSTALL_MODE_STORAGE_KEY]: 'reflink' })
    expect(resolveInstallMode(storage)).toBe('shared')
  })

  it('reflects a write made after it was first called', () => {
    const storage = createStorage()
    expect(resolveInstallMode(storage)).toBe('shared')
    saveInstallMode('copy', storage)
    expect(resolveInstallMode(storage)).toBe('copy')
  })
})

describe('defaults', () => {
  it('defaults to shared install', () => {
    const fallback: InstallMode = readStoredInstallMode(createStorage()) ?? DEFAULT_INSTALL_MODE
    expect(fallback).toBe('shared')
  })
})
