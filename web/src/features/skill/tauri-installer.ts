import { invokeTauri } from '@/shared/lib/tauri'
import type { InstallMode } from '@/shared/lib/install-mode'

/** An install target (agent) exposed by the desktop `detect_agents` command. */
export interface AgentTarget {
  id: string
  name: string
  dir: string
  installed: boolean
}

/** How a skill entry exists on disk inside an agent's skills root. */
export type LocationKind = 'dir' | 'symlink' | 'broken-symlink' | 'foreign-symlink'

/** Whether skillhub installed this skill, and can therefore update it. */
export type SkillOrigin = 'managed' | 'unmanaged'

/** Where a local skill's homepage link came from. */
export type HomepageSource = 'frontmatter' | 'metadata' | 'registry'

/** One way a skill is reachable from an agent's skills root. */
export interface SkillLocation {
  agent: string
  /** The entry itself: the link when it is a link, the directory otherwise. */
  path: string
  kind: LocationKind
  /** The link target as written on disk; absent for a plain directory. */
  target?: string
}

/**
 * A locally installed skill, aggregated across every agent root it appears in.
 *
 * One real directory reachable from several agents yields one `LocalSkill` with
 * several `locations`, rather than one entry per agent.
 */
export interface LocalSkill {
  /** Directory name; the display identity when there is no metadata. */
  slug: string
  namespace?: string
  version?: string
  registry?: string
  /** Set only when metadata records a different slug than the directory name. */
  metadataSlug?: string
  origin: SkillOrigin
  /** Canonical directory the skill actually lives in; absent for broken links. */
  realPath?: string
  locations: SkillLocation[]
  homepage?: string
  homepageSource?: HomepageSource
  /** The entry exists but could not be resolved (permissions, I/O error). */
  unreadable: boolean
}

/** Payload of `list_installed_skills`. */
export interface LocalSkillsPayload {
  skills: LocalSkill[]
  /** Skills roots that could not be read, so the UI can say so. */
  warnings: string[]
}

/** Payload accepted by the desktop `install_skill_command`. */
export interface InstallSkillInput {
  namespace: string
  slug: string
  version: string
  agent: string
  dir?: string
  /** When a same-named non-skillhub dir exists: true = back it up, false = overwrite. */
  preserveExisting?: boolean
  /**
   * Shared vs independent-copy install.
   *
   * Resolve this at click time with `resolveInstallMode()` rather than closing
   * over a hook value, so a mode switched moments earlier is the one that takes
   * effect. Omitting it makes Rust default to `shared`.
   */
  installMode?: InstallMode
}

/** Result of an install, returned by the desktop `install_skill_command`. */
export interface InstallSkillResult {
  ok: boolean
  /** Where the files landed — the link's target for a symlinked skill. */
  dir: string
  agent: string
  warnings: string[]
}

/** Per-agent install status for a skill, from `detect_skill_status`. */
export interface AgentSkillStatus {
  agent: string
  installed: boolean
  version: string
  outdated: boolean
  /** True when a same-named non-skillhub entry exists (manual skill). */
  unmanaged: boolean
  /** How the entry exists on disk, when it does at all. */
  kind?: LocationKind
}

/** Result of removing one skill location. */
export interface UninstallResult {
  ok: boolean
  agent?: string
  /** The entry that was removed (the link itself, when it was a link). */
  dir: string
  removedKind: LocationKind
  /** Set when only a link was removed: the surviving real directory. */
  realPathKept?: string
  /** Backup path when a non-skillhub dir was preserved instead of deleted. */
  backupDir?: string
}

/** Envelope returned by every desktop command. */
interface CommandResult<T> {
  ok: boolean
  data?: T
  error?: string
}

/**
 * Enumerate known local agents via the Tauri shell.
 * Returns `null` when not running inside the desktop app.
 */
export async function detectAgents(): Promise<AgentTarget[] | null> {
  const result = await invokeTauri<CommandResult<AgentTarget[]>>('detect_agents')
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '检测本地 agent 失败')
  }
  return result.data ?? []
}

/**
 * Install a skill to a local agent directory via the Tauri shell.
 *
 * When the target entry is a symlink, the files are written to the link's
 * target and the link is left in place.
 *
 * `registry` is the SkillHub registry base URL used to build the download URL;
 * it is resolved from the runtime config / browser origin by the caller.
 * Returns `null` when not running inside the desktop app.
 */
export async function installSkill(
  input: InstallSkillInput,
  registry: string,
): Promise<InstallSkillResult | null> {
  const result = await invokeTauri<CommandResult<InstallSkillResult>>(
    'install_skill_command',
    { input, registry },
  )
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '安装失败')
  }
  return result.data ?? { ok: false, dir: '', agent: input.agent, warnings: [] }
}

/**
 * Report per-agent install status for a skill (installed + local version).
 * Returns `null` when not running inside the desktop app.
 */
export async function detectSkillStatus(
  slug: string,
  version: string,
): Promise<AgentSkillStatus[] | null> {
  const result = await invokeTauri<CommandResult<AgentSkillStatus[]>>('detect_skill_status', {
    slug,
    version,
  })
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '检测本地已安装状态失败')
  }
  return result.data ?? []
}

/**
 * Remove one skill location, addressed by its own path.
 *
 * A skill reachable from several agents has several locations, so the caller
 * names the exact one to detach. Removing a symlink removes only the link.
 * Returns `null` when not running inside the desktop app.
 */
export async function uninstallSkill(
  dir: string,
  agent?: string,
): Promise<UninstallResult | null> {
  const result = await invokeTauri<CommandResult<UninstallResult>>('uninstall_skill_command', {
    dir,
    agent,
  })
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '卸载失败')
  }
  return result.data ?? { ok: false, dir, removedKind: 'dir' }
}

/** Payload accepted by the desktop `attach_skill_to_agent_command`. */
export interface AttachSkillInput {
  /** The skill's real directory — not a link path. */
  sourceDir: string
  agent: string
  mode: InstallMode
}

/** Result of attaching an existing skill to another agent. */
export interface AttachSkillResult {
  ok: boolean
  agent: string
  /** The agent-side entry that now exposes the skill. */
  dir: string
  /** The real directory the content lives in. */
  realDir: string
  warnings: string[]
}

/**
 * Make an already-installed skill available to another agent.
 *
 * Purely local: nothing is downloaded and no registry is consulted, so it works
 * for skills that were never installed from a registry. `mode` decides whether
 * the agent entry becomes a link to the real directory or its own copy.
 *
 * Returns `null` when not running inside the desktop app.
 */
export async function attachSkillToAgent(
  input: AttachSkillInput,
): Promise<AttachSkillResult | null> {
  const result = await invokeTauri<CommandResult<AttachSkillResult>>(
    'attach_skill_to_agent_command',
    { sourceDir: input.sourceDir, agent: input.agent, mode: input.mode },
  )
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '添加到其他 Agent 失败')
  }
  return (
    result.data ?? {
      ok: false,
      agent: input.agent,
      dir: '',
      realDir: input.sourceDir,
      warnings: [],
    }
  )
}

/**
 * Reveal a directory in the operating system's file manager.
 * Returns `null` when not running inside the desktop app.
 */
export async function openInFileManager(dir: string): Promise<void> {
  const result = await invokeTauri<CommandResult<null>>('open_directory', { dir })
  if (!result) {
    return
  }
  if (!result.ok) {
    throw new Error(result.error ?? '打开目录失败')
  }
}

/**
 * Open a URL in the operating system's default browser.
 *
 * The URL is validated to be http/https on the Rust side before it reaches the
 * OS, so a hostile `SKILL.md` cannot smuggle in a `javascript:` link.
 * Returns `null` when not running inside the desktop app.
 */
export async function openExternalUrl(url: string): Promise<void> {
  const result = await invokeTauri<CommandResult<null>>('open_external_url', { url })
  if (!result) {
    return
  }
  if (!result.ok) {
    throw new Error(result.error ?? '打开链接失败')
  }
}

/**
 * List every skill found across the local agent skills roots, managed or not.
 * Returns `null` when not running inside the desktop app.
 */
export async function listInstalledSkills(): Promise<LocalSkillsPayload | null> {
  const result =
    await invokeTauri<CommandResult<LocalSkillsPayload>>('list_installed_skills')
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '读取本地已安装技能失败')
  }
  return result.data ?? { skills: [], warnings: [] }
}
