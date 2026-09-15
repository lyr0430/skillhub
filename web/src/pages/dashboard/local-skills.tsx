import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AlertTriangle, Search } from 'lucide-react'
import { toast } from '@/shared/lib/toast'
import { isTauri } from '@/shared/lib/tauri'
import { copyToClipboard } from '@/shared/lib/clipboard'
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
import { LocalSkillCard } from '@/features/skill/local-skill-card'
import {
  detectAgents,
  listInstalledSkills,
  uninstallSkill,
  installSkill,
  openInFileManager,
  openExternalUrl,
  type AgentTarget,
  type LocalSkill,
  type SkillLocation,
} from '@/features/skill/tauri-installer'
import { getBaseUrl } from '@/features/skill/install-command'
import { resolveSkillVersion } from '@/api/client'

const PAGE_SIZE = 8

/** i18n key describing what removing a location of this kind actually does. */
function removalEffectKey(location: SkillLocation): string {
  switch (location.kind) {
    case 'symlink':
      return 'localSkills.effectSymlink'
    case 'broken-symlink':
    case 'foreign-symlink':
      return 'localSkills.effectBroken'
    case 'dir':
      return 'localSkills.effectDir'
  }
}

export function LocalSkillsPage() {
  const { t } = useTranslation()
  const [agents, setAgents] = useState<AgentTarget[]>([])
  /** null means "every agent"; the default, since a skill can live in several. */
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null)
  const [skills, setSkills] = useState<LocalSkill[]>([])
  const [warnings, setWarnings] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [confirmSkill, setConfirmSkill] = useState<LocalSkill | null>(null)
  const [confirmLocation, setConfirmLocation] = useState<SkillLocation | null>(null)
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(0)

  /**
   * Re-read the local skills. `silent` keeps the current list rendered while
   * the new one loads: blanking to skeletons after every uninstall would throw
   * away the user's scroll position and drop keyboard focus to the body, since
   * the card that had focus unmounts.
   */
  const refresh = useCallback(
    async (options?: { silent?: boolean }) => {
      if (!isTauri()) {
        setLoading(false)
        return
      }
      if (!options?.silent) {
        setLoading(true)
      }
      try {
        const targets = await detectAgents()
        const payload = await listInstalledSkills()
        setAgents(targets ?? [])
        setSkills(payload?.skills ?? [])
        setWarnings(payload?.warnings ?? [])
      } catch (err) {
        toast.error(
          t('localSkills.loadError'),
          err instanceof Error ? err.message : undefined,
        )
      } finally {
        setLoading(false)
      }
    },
    [t],
  )

  useEffect(() => {
    void refresh()
    // `refresh` only changes when `t` does; the optional arg is not a dep.
  }, [refresh])

  const openConfirm = (skill: LocalSkill) => {
    setConfirmSkill(skill)
    // With more than one location the choice is consequential — detaching one
    // agent's link versus deleting the real directory every agent shares — so
    // nothing is preselected and the confirm button stays disabled until the
    // user actually picks.
    setConfirmLocation(skill.locations.length === 1 ? skill.locations[0] : null)
  }

  const closeConfirm = () => {
    setConfirmSkill(null)
    setConfirmLocation(null)
  }

  const handleUninstall = async (location: SkillLocation) => {
    setBusyId(`${location.agent}:${location.path}`)
    try {
      const result = await uninstallSkill(location.path, location.agent)
      if (result === null) return
      // Say what actually happened: a link removal leaves the real directory
      // behind, and an unmanaged directory is renamed rather than deleted.
      const detail = result.realPathKept
        ? t('localSkills.uninstallLinkKept', { path: result.realPathKept })
        : result.backupDir
          ? t('localSkills.uninstallBackup', { dir: result.backupDir })
          : result.dir
      toast.success(t('localSkills.uninstallSuccess'), detail)
      await refresh({ silent: true })
    } catch (err) {
      toast.error(
        t('localSkills.uninstallError'),
        err instanceof Error ? err.message : undefined,
      )
    } finally {
      setBusyId(null)
    }
  }

  const handleUpdate = async (skill: LocalSkill) => {
    const location = skill.locations[0]
    if (!location) return
    setBusyId(`${location.agent}:${location.path}`)
    const registry = skill.registry || getBaseUrl()
    // The directory on disk may be named differently from the published slug;
    // the download uses the published coordinate while `dir` names the actual
    // entry, so a renamed skill still updates in place.
    const publishedSlug = skill.metadataSlug ?? skill.slug
    try {
      const resolved = await resolveSkillVersion(skill.namespace ?? '', publishedSlug)
      const latest = resolved?.version
      if (latest && latest === skill.version) {
        toast.warning(t('localSkills.alreadyLatest'), `${location.agent} · ${skill.version}`)
        return
      }
      const result = await installSkill(
        {
          namespace: skill.namespace ?? '',
          slug: publishedSlug,
          version: latest || skill.version || '',
          agent: location.agent,
          dir: location.path,
        },
        registry,
      )
      if (result === null) return
      const throughLink = result.warnings.find((warning) => warning.includes('软链接'))
      toast.success(t('localSkills.updateSuccess'), throughLink ?? result.dir)
      await refresh({ silent: true })
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
      await copyToClipboard(dir)
      toast.success(t('localSkills.pathCopied'))
    } catch (err) {
      toast.error(
        t('localSkills.copyError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  const handleOpenFolder = async (dir: string) => {
    try {
      await openInFileManager(dir)
    } catch (err) {
      toast.error(
        t('localSkills.openInFolderError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  const handleOpenHomepage = async (url: string) => {
    try {
      await openExternalUrl(url)
    } catch (err) {
      toast.error(
        t('localSkills.openHomepageError'),
        err instanceof Error ? err.message : undefined,
      )
    }
  }

  // Skills reachable from the selected agent (or all of them when none is picked).
  const visibleSkills = useMemo(() => {
    if (!selectedAgent) return skills
    return skills.filter((skill) =>
      skill.locations.some((location) => location.agent === selectedAgent),
    )
  }, [skills, selectedAgent])

  const filteredSkills = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return visibleSkills
    return visibleSkills.filter((skill) => {
      const haystack = [
        skill.namespace ?? '',
        skill.slug,
        skill.metadataSlug ?? '',
        skill.version ?? '',
        skill.origin,
        skill.realPath ?? '',
        ...skill.locations.flatMap((location) => [location.agent, location.path, location.kind]),
      ]
        .join(' ')
        .toLowerCase()
      return haystack.includes(q)
    })
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

      {/* Agent filter. "All" leads, because a skill shared between agents has
          more than one home and hiding it behind an agent filter would be a lie. */}
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          data-testid="local-agent-all"
          onClick={() => {
            setSelectedAgent(null)
            setPage(0)
          }}
          aria-pressed={selectedAgent === null}
          className={cn(
            'rounded-xl border px-3 py-2 text-sm font-medium transition-colors',
            selectedAgent === null
              ? 'border-primary/60 bg-primary/10 text-foreground'
              : 'border-border/60 bg-muted/40 text-foreground hover:bg-muted/70',
          )}
        >
          {t('localSkills.allAgents')}
        </button>

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

      {/* Roots that could not be read. Silent truncation would look like "you
          have fewer skills than you do". */}
      {warnings.length > 0 && (
        <Card className="flex items-start gap-3 border-amber-500/40 bg-amber-500/5 p-3">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
          <p className="text-xs text-muted-foreground">{warnings.join(' · ')}</p>
        </Card>
      )}

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
                {pagedSkills.map((skill) => (
                  <LocalSkillCard
                    key={skill.realPath ?? `${skill.slug}:${skill.locations[0]?.path ?? ''}`}
                    skill={skill}
                    busy={skill.locations.some(
                      (location) => busyId === `${location.agent}:${location.path}`,
                    )}
                    onCopyPath={handleCopyPath}
                    onOpenFolder={handleOpenFolder}
                    onOpenHomepage={handleOpenHomepage}
                    onUninstall={openConfirm}
                    onUpdate={handleUpdate}
                  />
                ))}
              </div>
              {totalPages > 1 && (
                <Pagination page={safePage} totalPages={totalPages} onPageChange={setPage} />
              )}
            </>
          )}
        </>
      )}

      {/* Uninstall confirmation. When a skill is reachable from several agents,
          the user picks which one to detach — the consequences differ. */}
      <AlertDialog
        open={confirmSkill !== null}
        onOpenChange={(next) => !next && closeConfirm()}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('localSkills.confirmUninstallTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {confirmSkill &&
                t('localSkills.confirmUninstallDesc', {
                  skill: confirmSkill.namespace
                    ? `@${confirmSkill.namespace}/${confirmSkill.slug}`
                    : confirmSkill.slug,
                })}
            </AlertDialogDescription>
          </AlertDialogHeader>

          {confirmSkill && confirmSkill.locations.length > 1 && (
            <div className="space-y-2">
              <p className="text-sm text-foreground">{t('localSkills.chooseLocation')}</p>
              {confirmSkill.locations.map((location) => (
                <label
                  key={location.path}
                  data-testid={`local-location-${location.agent}`}
                  className={cn(
                    'flex cursor-pointer items-start gap-3 rounded-lg border p-3 text-sm transition-colors',
                    confirmLocation?.path === location.path
                      ? 'border-primary/60 bg-primary/10'
                      : 'border-border/60 bg-muted/30 hover:bg-muted/60',
                  )}
                >
                  <input
                    type="radio"
                    name="local-skill-location"
                    className="mt-0.5"
                    checked={confirmLocation?.path === location.path}
                    onChange={() => setConfirmLocation(location)}
                  />
                  <span className="min-w-0">
                    <span className="font-medium text-foreground">{location.agent}</span>
                    <span className="mt-0.5 block truncate font-mono text-xs text-muted-foreground">
                      {location.path}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          )}

          {confirmLocation && (
            <p className="rounded-lg bg-muted/50 p-3 text-xs text-muted-foreground">
              {t(removalEffectKey(confirmLocation), {
                path: confirmSkill?.realPath ?? confirmLocation.target ?? confirmLocation.path,
                name: confirmSkill?.slug ?? '',
              })}
            </p>
          )}

          <AlertDialogFooter>
            <Button type="button" variant="outline" onClick={closeConfirm}>
              {t('localSkills.cancel')}
            </Button>
            <Button
              type="button"
              variant="destructive"
              data-testid="confirm-uninstall"
              disabled={!confirmLocation}
              onClick={async () => {
                const location = confirmLocation
                closeConfirm()
                if (location) await handleUninstall(location)
              }}
            >
              {t('localSkills.uninstallConfirm')}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
