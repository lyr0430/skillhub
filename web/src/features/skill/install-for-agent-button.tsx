import { useEffect, useState } from 'react'
import { Bot, Check, Loader2, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useCopyToClipboard } from '@/shared/lib/clipboard'
import { isTauri } from '@/shared/lib/tauri'
import { toast } from '@/shared/lib/toast'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/dialog'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/alert-dialog'
import { Button } from '@/shared/ui/button'
import { cn } from '@/shared/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/shared/ui/tooltip'
import { AgentBrandIcon } from './agent-icons'
import { buildSkillhubCoordinate, getBaseUrl, isPortableSkillVersion } from './install-command'
import {
  detectAgents,
  detectSkillStatus,
  installSkill,
  uninstallSkill,
  type AgentTarget,
} from './tauri-installer'

interface InstallForAgentButtonProps {
  namespace: string
  slug: string
  version: string
  disabled?: boolean
}

type FormatAgentPrompt = (guideUrl: string, skill: string, version: string) => string

type Action = 'idle' | 'installing' | 'uninstalling'

/** Agents plus the per-agent install status (installed + local version). */
interface AgentEntry extends AgentTarget {
  installedVersion?: string
  outdated?: boolean
  /** True when a same-named non-skillhub dir exists for this agent (manual skill). */
  unmanaged?: boolean
}

/**
 * Shared prompt builder (kept for the web copy-to-clipboard fallback and tests).
 */
export function buildAgentInstallPrompt(
  namespace: string,
  slug: string,
  version: string,
  baseUrl: string,
  formatPrompt: FormatAgentPrompt,
): string {
  const skill = buildSkillhubCoordinate(namespace, slug)
  const guideUrl = `${baseUrl.replace(/\/+$/, '')}/registry/skill.md`
  return formatPrompt(guideUrl, skill, version)
}

export function InstallForAgentButton({
  namespace,
  slug,
  version,
  disabled = false,
}: InstallForAgentButtonProps) {
  const { t } = useTranslation()
  const [copied, copy] = useCopyToClipboard()
  const [open, setOpen] = useState(false)
  const [agents, setAgents] = useState<AgentEntry[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [action, setAction] = useState<Action>('idle')
  const [confirmUninstall, setConfirmUninstall] = useState(false)
  const [conflictOpen, setConflictOpen] = useState(false)

  const disabledForInstall = disabled || !isPortableSkillVersion(version)

  // Desktop mode: lazy-load the target list + per-agent install status.
  useEffect(() => {
    if (!open || !isTauri()) {
      return
    }
    let active = true
    ;(async () => {
      try {
        const targets = await detectAgents()
        if (!active) return
        const statuses = await detectSkillStatus(slug, version)
        if (!active) return
        const statusByAgent = new Map((statuses ?? []).map((s) => [s.agent, s]))
        const merged: AgentEntry[] = (targets ?? []).map((target) => {
          const st = statusByAgent.get(target.id)
          return {
            ...target,
            installedVersion: st?.version,
            outdated: st?.outdated,
            unmanaged: st?.unmanaged,
          }
        })
        setAgents(merged)
        setSelectedId((prev) => prev ?? merged[0]?.id ?? null)
      } catch (err) {
        if (!active) return
        toast.error(
          t('skillDetail.installForAgent.loadError'),
          err instanceof Error ? err.message : undefined,
        )
      }
    })()
    return () => {
      active = false
    }
  }, [open, slug, version, t])

  const handleCopy = async () => {
    try {
      await copy(
        buildAgentInstallPrompt(
          namespace,
          slug,
          version,
          getBaseUrl(),
          (guideUrl, skill, selectedVersion) =>
            t('skillDetail.installForAgent.prompt', { guideUrl, skill, version: selectedVersion }),
        ),
      )
    } catch (err) {
      console.error('Failed to copy agent installation prompt:', err)
    }
  }

  const handleInstall = async (preserveExisting: boolean) => {
    if (!selectedId) return
    setAction('installing')
    try {
      const result = await installSkill(
        { namespace, slug, version, agent: selectedId, preserveExisting },
        getBaseUrl(),
      )
      if (result === null) {
        // Not in the desktop shell — fall back to copy.
        setAction('idle')
        await handleCopy()
        return
      }
      const backupWarning = result.warnings.find((w) => w.includes('备份'))
      toast.success(
        t('skillDetail.installForAgent.installSuccess'),
        backupWarning ?? result.dir,
      )
      setOpen(false)
      setAction('idle')
    } catch (err) {
      setAction('idle')
      toast.error(
        t('skillDetail.installForAgent.installError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  // Entry point for the install/update buttons: if a same-named non-skillhub
  // directory exists, ask the user whether to overwrite or back it up first.
  const handleInstallClick = () => {
    if (action !== 'idle' || !selectedId) return
    if (selectedAgent?.unmanaged) {
      setConflictOpen(true)
      return
    }
    void handleInstall(false)
  }

  const handleUninstall = async () => {
    if (!selectedId) return
    setAction('uninstalling')
    try {
      const result = await uninstallSkill(selectedId, slug)
      if (result === null) return
      toast.success(
        t('skillDetail.installForAgent.uninstallSuccess'),
        result.backupDir
          ? t('skillDetail.installForAgent.uninstallBackup', { dir: result.backupDir })
          : result.dir,
      )
      // Refresh status so the agent no longer shows as installed.
      await refreshStatus()
      setAction('idle')
    } catch (err) {
      setAction('idle')
      toast.error(
        t('skillDetail.installForAgent.uninstallError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  const refreshStatus = async () => {
    const statuses = await detectSkillStatus(slug, version)
    if (statuses === null) return
    const statusByAgent = new Map(statuses.map((s) => [s.agent, s]))
    setAgents((current) =>
      current.map((target) => {
        const st = statusByAgent.get(target.id)
        return {
          ...target,
          installedVersion: st?.version,
          outdated: st?.outdated,
          unmanaged: st?.unmanaged,
        }
      }),
    )
  }

  const handleClick = () => {
    if (isTauri()) {
      setAction('idle')
      setOpen(true)
      return
    }
    void handleCopy()
  }

  const label = isTauri()
    ? t('skillDetail.installForAgent.button')
    : copied
      ? t('skillDetail.installForAgent.copied')
      : t('skillDetail.installForAgent.button')

  const selectedAgent = agents.find((agent) => agent.id === selectedId)
  const selectedInstalled = Boolean(selectedAgent?.installedVersion)

  if (isTauri()) {
    return (
      <>
        <button
          type="button"
          data-testid="install-for-agent-button"
          onClick={handleClick}
          disabled={disabledForInstall}
          aria-label={label}
          className="relative w-full overflow-hidden rounded-xl border border-border/60 bg-muted/50 px-4 py-3 transition-colors hover:bg-muted/70 active:bg-muted/80 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <div className="flex items-center justify-center gap-2">
            <Bot className="h-4 w-4" />
            <span className="text-[13px] leading-relaxed text-foreground sm:text-sm">{label}</span>
          </div>
        </button>

        <Dialog open={open} onOpenChange={(next) => setOpen(next)}>
          <DialogContent className="sm:max-w-xl">
            <DialogHeader>
              <DialogTitle>{t('skillDetail.installForAgent.dialogTitle')}</DialogTitle>
              <DialogDescription>
                {t('skillDetail.installForAgent.dialogDescription', {
                  skill: buildSkillhubCoordinate(namespace, slug),
                })}
              </DialogDescription>
            </DialogHeader>

            <TooltipProvider delayDuration={150}>
              <div className="flex flex-wrap items-center justify-center gap-3 py-2">
                {agents.map((agent) => {
                  if (action !== 'idle') {
                    return null
                  }
                  const selected = selectedId === agent.id
                  const isInstalled = Boolean(agent.installedVersion)
                  return (
                    <Tooltip key={agent.id}>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          data-testid={`install-target-${agent.id}`}
                          onClick={() => setSelectedId(agent.id)}
                          aria-pressed={selected}
                          aria-label={agent.name}
                          title={agent.name}
                          className={cn(
                            'relative flex h-20 w-24 flex-col items-center justify-center gap-1.5 rounded-xl border px-1.5 py-2 transition-all duration-150',
                            selected
                              ? 'border-primary/60 bg-primary/10 shadow-[inset_0_1px_0_0_hsl(0_0%_100%/0.10),0_0_0_1px_hsl(var(--primary)/0.10)]'
                              : 'border-border/60 bg-muted/40 hover:border-border hover:bg-muted/70',
                          )}
                        >
                          <span className="text-foreground">
                            <AgentBrandIcon id={agent.id} size={26} />
                          </span>
                          <span className="max-w-full line-clamp-2 text-center text-[10px] leading-tight text-foreground/90">
                            {agent.name}
                          </span>
                          {isInstalled && (
                            <span className="absolute right-1.5 top-1.5 h-2 w-2 rounded-full bg-emerald-500" />
                          )}
                        </button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom" sideOffset={6}>
                        <span className="block font-medium">{agent.name}</span>
                        <span className="block max-w-[16rem] truncate font-mono text-muted-foreground">
                          {agent.dir}
                        </span>
                        {isInstalled
                          ? (
                            <span className="block text-emerald-400">
                              {t('skillDetail.installForAgent.installedVersion', {
                                version: agent.installedVersion,
                              })}
                            </span>
                          )
                          : (
                            <span className="block text-muted-foreground">
                              {t('skillDetail.installForAgent.notInstalled')}
                            </span>
                          )}
                      </TooltipContent>
                    </Tooltip>
                  )
                })}

                {action === 'idle' && agents.length === 0 && (
                  <p className="w-full py-4 text-center text-sm text-muted-foreground">
                    {t('skillDetail.installForAgent.noAgents')}
                  </p>
                )}
              </div>
            </TooltipProvider>

            {action !== 'idle' && (
              <div className="flex items-center justify-center gap-2 py-4 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {action === 'uninstalling'
                  ? t('skillDetail.installForAgent.uninstalling')
                  : t('skillDetail.installForAgent.installing')}
              </div>
            )}

            <div className="flex items-center justify-end gap-3">
              <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
                {t('skillDetail.installForAgent.cancel')}
              </Button>

              {selectedInstalled && action === 'idle' && (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    data-testid="install-uninstall"
                    onClick={() => setConfirmUninstall(true)}
                  >
                    <Trash2 className="h-4 w-4" />
                    {t('skillDetail.installForAgent.uninstall')}
                  </Button>
                  {selectedAgent?.outdated && (
                    <Button
                      type="button"
                      variant="outline"
                      data-testid="install-update"
                      onClick={handleInstallClick}
                    >
                      <RefreshCw className="h-4 w-4" />
                      {t('skillDetail.installForAgent.update')}
                    </Button>
                  )}
                </>
              )}

              {(selectedAgent?.outdated || !selectedInstalled) && (
                <Button
                  type="button"
                  data-testid="install-confirm"
                  disabled={!selectedId || action !== 'idle'}
                  onClick={handleInstallClick}
                >
                  {selectedAgent?.outdated
                    ? t('skillDetail.installForAgent.update')
                    : t('skillDetail.installForAgent.confirm')}
                </Button>
              )}
            </div>
          </DialogContent>
        </Dialog>

        {/* Uninstall confirmation */}
        <AlertDialog open={confirmUninstall} onOpenChange={setConfirmUninstall}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t('skillDetail.installForAgent.confirmUninstallTitle')}</AlertDialogTitle>
              <AlertDialogDescription>
                {t('skillDetail.installForAgent.confirmUninstallDesc', {
                  skill: buildSkillhubCoordinate(namespace, slug),
                  agent: selectedAgent?.name ?? '',
                })}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <Button type="button" variant="outline" onClick={() => setConfirmUninstall(false)}>
                {t('skillDetail.installForAgent.cancel')}
              </Button>
              <Button
                type="button"
                variant="destructive"
                data-testid="confirm-install-uninstall"
                onClick={() => {
                  setConfirmUninstall(false)
                  void handleUninstall()
                }}
              >
                <Trash2 className="h-4 w-4" />
                {t('skillDetail.installForAgent.uninstallConfirm')}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        {/* Conflict: a same-named non-skillhub dir exists — choose overwrite or backup */}
        <AlertDialog open={conflictOpen} onOpenChange={setConflictOpen}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t('skillDetail.installForAgent.conflictTitle')}</AlertDialogTitle>
              <AlertDialogDescription>
                {t('skillDetail.installForAgent.conflictDesc', {
                  skill: buildSkillhubCoordinate(namespace, slug),
                  agent: selectedAgent?.name ?? '',
                })}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <Button
                type="button"
                variant="outline"
                data-testid="install-overwrite"
                onClick={() => {
                  setConflictOpen(false)
                  void handleInstall(false)
                }}
              >
                {t('skillDetail.installForAgent.overwrite')}
              </Button>
              <Button
                type="button"
                data-testid="install-backup"
                onClick={() => {
                  setConflictOpen(false)
                  void handleInstall(true)
                }}
              >
                {t('skillDetail.installForAgent.backupAndInstall')}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </>
    )
  }

  // Web (browser) fallback: copy the install prompt, preserving current behaviour.
  return (
    <button
      type="button"
      data-testid="install-for-agent-button"
      onClick={handleClick}
      disabled={disabledForInstall}
      aria-label={label}
      className="relative w-full overflow-hidden rounded-xl border border-border/60 bg-muted/50 px-4 py-3 transition-colors hover:bg-muted/70 active:bg-muted/80 disabled:cursor-not-allowed disabled:opacity-50"
    >
      <div className="flex items-center justify-center gap-2">
        {copied ? <Check className="h-4 w-4" /> : <Bot className="h-4 w-4" />}
        <span className="text-[13px] leading-relaxed text-foreground sm:text-sm">{label}</span>
      </div>
    </button>
  )
}
