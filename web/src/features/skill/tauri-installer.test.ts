import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  downloadSkillZip,
  getSkillStoragePath,
  resetSkillStoragePath,
  revealInFileManager,
  setSkillStoragePath,
} from './tauri-installer'

const hoisted = vi.hoisted(() => {
  const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = []
  let result: unknown = { ok: true, data: { path: '/Downloads/x.zip', filename: 'x.zip' } }
  return {
    calls,
    getResult: () => result,
    setResult: (next: unknown) => {
      result = next
    },
  }
})

vi.mock('@/shared/lib/tauri', () => ({
  isTauri: () => true,
  invokeTauri: (cmd: string, args?: Record<string, unknown>) => {
    hoisted.calls.push({ cmd, args })
    return Promise.resolve(hoisted.getResult())
  },
}))

describe('downloadSkillZip', () => {
  beforeEach(() => {
    hoisted.calls.length = 0
    hoisted.setResult({ ok: true, data: { path: '/Downloads/x.zip', filename: 'x.zip' } })
  })

  it('invokes the download command with the registry, coordinates, and chosen path', async () => {
    const result = await downloadSkillZip(
      'global',
      'my-skill',
      '1.2.3',
      'http://localhost:8080',
      '/Users/me/Downloads/my-skill-1.2.3.zip',
    )

    expect(result).toEqual({ path: '/Downloads/x.zip', filename: 'x.zip' })
    expect(hoisted.calls).toEqual([
      {
        cmd: 'download_skill_zip_command',
        args: {
          registry: 'http://localhost:8080',
          namespace: 'global',
          slug: 'my-skill',
          version: '1.2.3',
          path: '/Users/me/Downloads/my-skill-1.2.3.zip',
        },
      },
    ])
  })

  it('throws when the command reports an error', async () => {
    hoisted.setResult({ ok: false, error: '下载失败', data: undefined })

    await expect(
      downloadSkillZip('global', 'my-skill', '1.2.3', 'http://localhost:8080', '/tmp/x.zip'),
    ).rejects.toThrow('下载失败')
  })

  it('returns null when not running inside the desktop app', async () => {
    hoisted.setResult(null)

    await expect(
      downloadSkillZip('global', 'my-skill', '1.2.3', 'http://localhost:8080', '/tmp/x.zip'),
    ).resolves.toBeNull()
  })
})

describe('revealInFileManager', () => {
  beforeEach(() => {
    hoisted.calls.length = 0
    hoisted.setResult({ ok: true, data: null })
  })

  it('reveals the downloaded file path via reveal_path', async () => {
    await revealInFileManager('/Downloads/x.zip')

    expect(hoisted.calls).toEqual([{ cmd: 'reveal_path', args: { path: '/Downloads/x.zip' } }])
  })

  it('throws when the reveal command reports an error', async () => {
    hoisted.setResult({ ok: false, error: '打开文件管理器失败', data: undefined })

    await expect(revealInFileManager('/Downloads/x.zip')).rejects.toThrow('打开文件管理器失败')
  })
})

describe('skill storage path commands', () => {
  beforeEach(() => {
    hoisted.calls.length = 0
  })

  it('reads the current storage path', async () => {
    hoisted.setResult({ ok: true, data: '/default/skillhub/skills' })

    const path = await getSkillStoragePath()

    expect(path).toBe('/default/skillhub/skills')
    expect(hoisted.calls).toEqual([{ cmd: 'get_skill_storage_path' }])
  })

  it('relocates the repository via set_skill_storage_path', async () => {
    hoisted.setResult({
      ok: true,
      data: {
        ok: true,
        newPath: '/new/repo',
        movedSlugs: ['alpha'],
        updatedLinks: [],
        warnings: [],
      },
    })

    const result = await setSkillStoragePath('/new/repo')

    expect(result?.newPath).toBe('/new/repo')
    expect(hoisted.calls).toEqual([
      { cmd: 'set_skill_storage_path', args: { path: '/new/repo' } },
    ])
  })

  it('restores the default path', async () => {
    hoisted.setResult({
      ok: true,
      data: {
        ok: true,
        newPath: '/default/skillhub/skills',
        movedSlugs: [],
        updatedLinks: [],
        warnings: [],
      },
    })

    const result = await resetSkillStoragePath()

    expect(result?.newPath).toBe('/default/skillhub/skills')
    expect(hoisted.calls).toEqual([{ cmd: 'reset_skill_storage_path' }])
  })

  it('throws on a command-reported error', async () => {
    hoisted.setResult({ ok: false, error: '迁移技能存储路径失败', data: undefined })

    await expect(setSkillStoragePath('/x')).rejects.toThrow('迁移技能存储路径失败')
  })

  it('returns null when not running inside the desktop app', async () => {
    hoisted.setResult(null)

    await expect(getSkillStoragePath()).resolves.toBeNull()
    await expect(setSkillStoragePath('/x')).resolves.toBeNull()
    await expect(resetSkillStoragePath()).resolves.toBeNull()
  })
})
