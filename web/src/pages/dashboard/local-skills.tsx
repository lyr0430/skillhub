import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FolderOpen, Copy, RefreshCw, Search, Trash2 } from 'lucide-react'
import { toast } from '@/shared/lib/toast'
import { isTauri } from '@/shared/lib/tauri'
import { useCopyToClipboard } from '@/shared/lib/clipboard'
import { DashboardPageHeader } from '@/shared/components/dashboard-page-header'
import { Button } from '@/shared/ui/button'
import { Card } from '@/shared/ui/card'
import { Input } from '@/shared/ui/input'
import { Pagination } from '@/shared/components/pagination'
import { cn } from '@/shared/lib/utils'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/alert-dialog'
import { AgentBrandIcon } from '@/features/skill/agent-icons'
import {
  detectAgents,
  listInstalledSkills,
  uninstallSkill,
  installSkill,
  openInFileManager,
  type AgentTarget,
  type InstalledSkill,
} from '@/features/skill/tauri-installer'
import { getBaseUrl } from '@/features/skill/install-command'
import { resolveSkillVersion } from '@/api/client'

export function LocalSkillsPage() {
  const { t } = useTranslation()
  const [, copy] = useCopyToClipboard()
  const [agents, setAgents] = useState<AgentTarget[]>([])
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null)
  const [skills, setSkills] = useState<InstalledSkill[]>([])
  const [loading, setLoading] = useState(true)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [confirmSkill, setConfirmSkill] = useState<InstalledSkill | null>(null)
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(0)

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      setLoading(false)
      return
    }
    setLoading(true)
    try {
      const targets = await detectAgents()
      const list = await listInstalledSkills()
      setAgents(targets ?? [])
      setSkills(list ?? [])
      setSelectedAgent((prev) => prev ?? targets?.[0]?.id ?? null)
    } catch (err) {
      toast.error(
        t('localSkills.loadError'),
        err instanceof Error ? err.message : undefined,
      )
    } finally {
      setLoading(false)
    }
  }, [t])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const handleUninstall = async (skill: InstalledSkill) => {
    const key = skill.agent + '/' + skill.slug
    setBusyId(key)
    try {
      const result = await uninstallSkill(skill.agent, skill.slug)
      if (result === null) return
      toast.success(
        t('localSkills.uninstallSuccess'),
        // result.backupDir
        //   ? t('localSkills.uninstallBackup', { dir: result.backupDir })
        //   : result.dir,
      )
      await refresh()
    } catch (err) {
      toast.error(
        t('localSkills.uninstallError'),
        err instanceof Error ? err.message : undefined,
      )
    } finally {
      setBusyId(null)
    }
  }

  const handleUpdate = async (skill: InstalledSkill) => {
    const key = skill.agent + '/' + skill.slug
    setBusyId(key)
    const registry = skill.registry || getBaseUrl()
    try {
      const resolved = await resolveSkillVersion(skill.namespace, skill.slug)
      const latest = resolved?.version
      if (latest && latest === skill.version) {
        toast.warning(t('localSkills.alreadyLatest'), `${skill.agent} · ${skill.version}`)
        return
      }
      const result = await installSkill(
        {
          namespace: skill.namespace,
          slug: skill.slug,
          version: latest || skill.version,
          agent: skill.agent,
        },
        registry,
      )
      if (result === null) return
      toast.success(t('localSkills.updateSuccess'), result.dir)
      await refresh()
    } catch (err) {
      toast.error(
        t('localSkills.updateError'),
        err instanceof Error ? err.message : undefined,
      )
    } finally {
      setBusyId(null)
    }
  }

  const handleCopyPath = async (dir: string) => {
    try {
      await copy(dir)
      toast.success(t('localSkills.pathCopied'))
    } catch (err) {
      toast.error(
        t('localSkills.copyError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  const handleOpenFolder = async (skill: InstalledSkill) => {
    try {
      await openInFileManager(skill.dir)
    } catch (err) {
      toast.error(
        t('localSkills.openInFolderError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  // Skills belonging to the currently selected agent (fallback: show all).
  const visibleSkills = useMemo(() => {
    if (!selectedAgent) return skills
    return skills.filter((skill) => skill.agent === selectedAgent)
  }, [skills, selectedAgent])

  const PAGE_SIZE = 8

  // Narrow the agent-scoped list by an optional free-text query.
  const filteredSkills = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return visibleSkills
    return visibleSkills.filter(
      (skill) =>
        `${skill.namespace}/${skill.slug}`.toLowerCase().includes(q) ||
        skill.slug.toLowerCase().includes(q) ||
        skill.dir.toLowerCase().includes(q) ||
        skill.agent.toLowerCase().includes(q) ||
        (skill.version ?? '').toLowerCase().includes(q),
    )
  }, [visibleSkills, search])

  const totalPages = Math.max(1, Math.ceil(filteredSkills.length / PAGE_SIZE))

  // Keep `page` in range when the filter shrinks the result set.
  useEffect(() => {
    setPage((current) => Math.min(current, totalPages - 1))
  }, [totalPages])

  const safePage = Math.min(page, totalPages - 1)
  const pageStart = safePage * PAGE_SIZE
  const pagedSkills = useMemo(
    () => filteredSkills.slice(pageStart, pageStart + PAGE_SIZE),
    [filteredSkills, pageStart],
  )

  if (!isTauri()) {
    return (
      <div className="space-y-8 animate-fade-up">
        <DashboardPageHeader title={t('localSkills.title')} subtitle={t('localSkills.subtitle')} />
        <Card className="p-12 text-center text-muted-foreground">
          {t('localSkills.desktopOnly')}
        </Card>
      </div>
    )
  }

  return (
    <div className="space-y-8 animate-fade-up">
      <DashboardPageHeader title={t('localSkills.title')} subtitle={t('localSkills.subtitle')} />

      {/* Agent selector */}
      <div className="flex flex-wrap items-center gap-3">
        {agents.map((agent) => {
          const selected = selectedAgent === agent.id
          return (
            <button
              key={agent.id}
              type="button"
              data-testid={`local-agent-${agent.id}`}
              onClick={() => {
                setSelectedAgent(agent.id)
                setPage(0)
              }}
              aria-pressed={selected}
              aria-label={agent.name}
              className={cn(
                'flex items-center gap-2 rounded-xl border px-3 py-2 transition-colors',
                selected
                  ? 'border-primary/60 bg-primary/10'
                  : 'border-border/60 bg-muted/40 hover:bg-muted/70',
              )}
            >
              <AgentBrandIcon id={agent.id} size={18} />
              <span className="text-sm font-medium text-foreground">{agent.name}</span>
            </button>
          )
        })}
      </div>

      {loading ? (
        <div className="space-y-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-24 animate-shimmer rounded-xl" />
          ))}
        </div>
      ) : visibleSkills.length === 0 ? (
        <Card className="p-12 text-center text-muted-foreground">{t('localSkills.empty')}</Card>
      ) : (
        <>
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              type="search"
              value={search}
              onChange={(event) => {
                setSearch(event.target.value)
                setPage(0)
              }}
              placeholder={t('localSkills.searchPlaceholder')}
              aria-label={t('localSkills.searchPlaceholder')}
              className="pl-9"
            />
          </div>

          {filteredSkills.length === 0 ? (
            <Card className="p-12 text-center text-muted-foreground">{t('localSkills.noResults')}</Card>
          ) : (
            <>
              <div className="space-y-3">
                {pagedSkills.map((skill) => {
            const key = skill.agent + '/' + skill.slug
            const busy = busyId === key
            return (
              <Card key={key} className="p-4">
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <h3 className="truncate font-semibold text-foreground">
                        @{skill.namespace}/{skill.slug}
                      </h3>
                      <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
                        {skill.agent}
                      </span>
                    </div>
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">{skill.dir}</p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {t('localSkills.version', { version: skill.version })}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      aria-label={t('localSkills.copyPath')}
                      title={t('localSkills.copyPath')}
                      onClick={() => handleCopyPath(skill.dir)}
                    >
                      <Copy className="h-4 w-4" />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      data-testid={`local-open-${skill.agent}-${skill.slug}`}
                      aria-label={t('localSkills.openInFolder')}
                      title={t('localSkills.openInFolder')}
                      disabled={busy}
                      onClick={() => handleOpenFolder(skill)}
                    >
                      <FolderOpen className="h-4 w-4" />
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      data-testid={`local-uninstall-${skill.agent}-${skill.slug}`}
                      disabled={busy}
                      onClick={() => setConfirmSkill(skill)}
                    >
                      <Trash2 className="h-4 w-4" />
                      {t('localSkills.uninstall')}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      data-testid={`local-update-${skill.agent}-${skill.slug}`}
                      disabled={busy}
                      onClick={() => handleUpdate(skill)}
                    >
                      <RefreshCw className={busy ? 'h-4 w-4 animate-spin' : 'h-4 w-4'} />
                      {t('localSkills.update')}
                    </Button>
                  </div>
                </div>
              </Card>
            )
                })}
              </div>
              {totalPages > 1 && (
                <Pagination page={safePage} totalPages={totalPages} onPageChange={setPage} />
              )}
            </>
          )}
        </>
      )}

      {/* Uninstall confirmation dialog */}
      <AlertDialog open={confirmSkill !== null} onOpenChange={(next) => !next && setConfirmSkill(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('localSkills.confirmUninstallTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('localSkills.confirmUninstallDesc', {
                skill: confirmSkill ? `@${confirmSkill.namespace}/${confirmSkill.slug}` : '',
                agent: confirmSkill?.agent ?? '',
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button type="button" variant="outline" onClick={() => setConfirmSkill(null)}>
              {t('localSkills.cancel')}
            </Button>
            <Button
              type="button"
              variant="destructive"
              data-testid="confirm-uninstall"
              onClick={async () => {
                const skill = confirmSkill
                setConfirmSkill(null)
                if (skill) await handleUninstall(skill)
              }}
            >
              <Trash2 className="h-4 w-4" />
              {t('localSkills.uninstallConfirm')}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
