import { invokeTauri } from '@/shared/lib/tauri'

/** An install target (agent) exposed by the desktop `detect_agents` command. */
export interface AgentTarget {
  id: string
  name: string
  dir: string
  installed: boolean
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
}

/** Result of an install, returned by the desktop `install_skill_command`. */
export interface InstallSkillResult {
  ok: boolean
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
  /** True when a same-named non-skillhub directory exists (manual skill). */
  unmanaged: boolean
}

/** Result of an uninstall. */
export interface UninstallResult {
  ok: boolean
  agent: string
  dir: string
  /** Backup path when a non-skillhub dir was preserved instead of deleted. */
  backupDir?: string
}

/** A locally installed skill discovered on an agent's skill root. */
export interface InstalledSkill {
  registry: string
  namespace: string
  slug: string
  version: string
  agent: string
  dir: string
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
 * Uninstall a skill from a target agent directory via the Tauri shell.
 * Returns `null` when not running inside the desktop app.
 */
export async function uninstallSkill(
  agent: string,
  slug: string,
): Promise<UninstallResult | null> {
  const result = await invokeTauri<CommandResult<UninstallResult>>('uninstall_skill_command', {
    agent,
    slug,
  })
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '卸载失败')
  }
  return result.data ?? { ok: false, agent, dir: '' }
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
 * List all skillhub-installed skills across every agent.
 * Returns `null` when not running inside the desktop app.
 */
export async function listInstalledSkills(): Promise<InstalledSkill[] | null> {
  const result = await invokeTauri<CommandResult<InstalledSkill[]>>('list_installed_skills')
  if (!result) {
    return null
  }
  if (!result.ok) {
    throw new Error(result.error ?? '读取本地已安装技能失败')
  }
  return result.data ?? []
}
