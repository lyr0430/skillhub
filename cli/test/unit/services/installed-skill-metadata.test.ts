import { mkdir, mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, test } from 'bun:test'
import { readInstalledSkillMetadata } from '../../../src/services/installed-skill-metadata'

/**
 * Write a `.skillhub/metadata.json` holding `body` and read it back.
 */
async function readBack(body: string) {
  const dir = await mkdtemp(join(tmpdir(), 'skillhub-metadata-'))
  await mkdir(join(dir, '.skillhub'), { recursive: true })
  await writeFile(join(dir, '.skillhub', 'metadata.json'), body)
  return readInstalledSkillMetadata(dir)
}

/**
 * A metadata file exactly as the desktop client's Rust installer writes it
 * (`web/src-tauri/src/installer/metadata.rs`): version 1, source "skillhub",
 * camelCase `schemaVersion`, and optional fields omitted when absent.
 */
const DESKTOP_WRITTEN = JSON.stringify({
  schemaVersion: 1,
  registry: 'https://skill.example.com',
  namespace: 'global',
  slug: 'weather',
  version: '1.0.0',
  source: 'skillhub',
})

describe('readInstalledSkillMetadata', () => {
  test('accepts metadata written by the desktop client', async () => {
    const result = await readBack(DESKTOP_WRITTEN)

    expect(result.status).toBe('valid')
    if (result.status === 'valid') {
      expect(result.metadata.slug).toBe('weather')
      expect(result.metadata.version).toBe('1.0.0')
    }
  })

  test('accepts the reserved out-of-band homepage field the desktop client reads', async () => {
    // The desktop client reserves `homepage` for a skill whose SKILL.md carries
    // none. Nothing writes it yet, but a file that does carry it must not be
    // rejected — the two clients share this file.
    const result = await readBack(
      JSON.stringify({
        ...JSON.parse(DESKTOP_WRITTEN),
        homepage: 'https://github.com/team/weather',
      }),
    )

    expect(result.status).toBe('valid')
  })

  test('accepts the extra bookkeeping fields the CLI itself writes', async () => {
    const result = await readBack(
      JSON.stringify({
        ...JSON.parse(DESKTOP_WRITTEN),
        versionId: 42,
        fingerprint: 'abc123',
        files: { 'SKILL.md': 'hash' },
      }),
    )

    expect(result.status).toBe('valid')
  })

  test('rejects a truncated file rather than treating it as absent', async () => {
    // A half-written or hand-edited file must not read as "no metadata here":
    // that verdict is what lets a directory the UI promises to back up get
    // deleted instead.
    const result = await readBack('{"slug":"acme"}')

    expect(result.status).toBe('invalid')
  })

  test('rejects a file whose schema version is not supported', async () => {
    const result = await readBack(
      JSON.stringify({ ...JSON.parse(DESKTOP_WRITTEN), schemaVersion: 2 }),
    )

    expect(result.status).toBe('invalid')
  })
})
