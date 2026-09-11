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
}

/** Result of an install, returned by the desktop `install_skill_command`. */
export interface InstallSkillResult {
  ok: boolean
  dir: string
  agent: string
  warnings: string[]
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
