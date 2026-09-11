import { useEffect, useState } from 'react'
import { Bot, Check, Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useCopyToClipboard } from '@/shared/lib/clipboard'
import { isTauri } from '@/shared/lib/tauri'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/dialog'
import { Button } from '@/shared/ui/button'
import { cn } from '@/shared/lib/utils'
import { buildSkillhubCoordinate, getBaseUrl, isPortableSkillVersion } from './install-command'
import { detectAgents, installSkill, type AgentTarget } from './tauri-installer'

interface InstallForAgentButtonProps {
  namespace: string
  slug: string
  version: string
  disabled?: boolean
}

type FormatAgentPrompt = (guideUrl: string, skill: string, version: string) => string

type InstallStatus =
  | { state: 'idle' }
  | { state: 'loading' }
  | { state: 'success'; dir: string; agent: string; warnings: string[] }
  | { state: 'error'; message: string }

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
  const [agents, setAgents] = useState<AgentTarget[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [status, setStatus] = useState<InstallStatus>({ state: 'idle' })

  const disabledForInstall = disabled || !isPortableSkillVersion(version)

  // Desktop mode: lazy-load the target list when the dialog opens.
  useEffect(() => {
    if (!open || !isTauri()) {
      return
    }
    let active = true
    detectAgents()
      .then((targets) => {
        if (!active) return
        setAgents(targets ?? [])
        setSelectedId((prev) => prev ?? targets?.[0]?.id ?? null)
      })
      .catch((err: unknown) => {
        if (!active) return
        setStatus({ state: 'error', message: err instanceof Error ? err.message : String(err) })
      })
    return () => {
      active = false
    }
  }, [open])

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

  const handleInstall = async () => {
    if (!selectedId) {
      return
    }
    setStatus({ state: 'loading' })
    try {
      const result = await installSkill(
        { namespace, slug, version, agent: selectedId },
        getBaseUrl(),
      )
      if (result === null) {
        // Not in the desktop shell — fall back to copy.
        setStatus({ state: 'idle' })
        await handleCopy()
        return
      }
      setStatus({
        state: 'success',
        dir: result.dir,
        agent: result.agent,
        warnings: result.warnings ?? [],
      })
    } catch (err) {
      setStatus({
        state: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  const handleClick = () => {
    if (isTauri()) {
      setStatus({ state: 'idle' })
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
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>{t('skillDetail.installForAgent.dialogTitle')}</DialogTitle>
              <DialogDescription>
                {t('skillDetail.installForAgent.dialogDescription', {
                  skill: buildSkillhubCoordinate(namespace, slug),
                })}
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-2">
              {agents.map((agent) => {
                if (StatusIsLoading(status) || StatusIsSuccess(status) || StatusIsError(status)) {
                  return null
                }
                return (
                  <button
                    key={agent.id}
                    type="button"
                    data-testid={`install-target-${agent.id}`}
                    onClick={() => setSelectedId(agent.id)}
                    aria-pressed={selectedId === agent.id}
                    className={cn(
                      'flex w-full items-center justify-between rounded-xl border px-4 py-3 text-left transition-colors',
                      selectedId === agent.id
                        ? 'border-primary/60 bg-primary/5'
                        : 'border-border/60 bg-muted/40 hover:bg-muted/70',
                    )}
                  >
                    <span className="flex items-center gap-3">
                      <Bot className="h-4 w-4 text-muted-foreground" />
                      <span className="text-sm font-medium text-foreground">{agent.name}</span>
                    </span>
                    <span className="flex items-center gap-2 text-xs">
                      <span className="max-w-[12rem] truncate font-mono text-muted-foreground/70">
                        {agent.dir}
                      </span>
                      {agent.installed && (
                        <span className="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] text-emerald-600 dark:text-emerald-400">
                          {t('skillDetail.installForAgent.installed')}
                        </span>
                      )}
                    </span>
                  </button>
                )
              })}

              {StatusIsLoading(status) && (
                <div className="flex items-center justify-center gap-2 py-6 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('skillDetail.installForAgent.installing')}
                </div>
              )}

              {StatusIsSuccess(status) && (
                <div className="flex flex-col gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/5 p-4">
                  <div className="flex items-center gap-2 text-sm font-medium text-emerald-600 dark:text-emerald-400">
                    <Check className="h-4 w-4" />
                    {t('skillDetail.installForAgent.installed')}
                  </div>
                  <p className="break-all font-mono text-xs text-muted-foreground">{status.dir}</p>
                  {status.warnings.map((warning) => (
                    <p key={warning} className="text-xs text-amber-600 dark:text-amber-400">
                      {warning}
                    </p>
                  ))}
                </div>
              )}

              {StatusIsError(status) && (
                <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
                  {status.message}
                </div>
              )}
            </div>

            <div className="flex items-center justify-end gap-3">
              <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
                {t('skillDetail.installForAgent.cancel')}
              </Button>
              {status.state !== 'success' && (
                <Button
                  type="button"
                  data-testid="install-confirm"
                  disabled={!selectedId || status.state === 'loading'}
                  onClick={handleInstall}
                >
                  {status.state === 'loading'
                    ? t('skillDetail.installForAgent.installing')
                    : t('skillDetail.installForAgent.confirm')}
                </Button>
              )}
            </div>
          </DialogContent>
        </Dialog>
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

function StatusIsLoading(status: InstallStatus): status is Extract<InstallStatus, { state: 'loading' }> {
  return status.state === 'loading'
}
function StatusIsSuccess(status: InstallStatus): status is Extract<InstallStatus, { state: 'success' }> {
  return status.state === 'success'
}
function StatusIsError(status: InstallStatus): status is Extract<InstallStatus, { state: 'error' }> {
  return status.state === 'error'
}
