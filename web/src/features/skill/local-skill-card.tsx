import { useTranslation } from 'react-i18next'
import { Copy, ExternalLink, FolderOpen, Link2, Link2Off, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { Button } from '@/shared/ui/button'
import { Card } from '@/shared/ui/card'
import { cn } from '@/shared/lib/utils'
import { AgentBrandIcon } from './agent-icons'
import type { HomepageSource, LocalSkill, LocationKind } from './tauri-installer'

interface LocalSkillCardProps {
  skill: LocalSkill
  busy: boolean
  /**
   * Agents that could still receive this skill. The entry is hidden when there
   * are none, so the row never offers an action that would open an empty list.
   */
  attachTargetCount: number
  onCopyPath: (path: string) => void
  onOpenFolder: (path: string) => void
  onOpenHomepage: (url: string) => void
  onUninstall: (skill: LocalSkill) => void
  onUpdate: (skill: LocalSkill) => void
  onAttach: (skill: LocalSkill) => void
}

/** i18n key suffix for each on-disk shape, used for the location chip. */
const KIND_LABEL: Record<LocationKind, string | null> = {
  dir: null,
  symlink: 'localSkills.kindSymlink',
  'broken-symlink': 'localSkills.kindBroken',
  'foreign-symlink': 'localSkills.kindForeign',
}

const HOMEPAGE_SOURCE_LABEL: Record<HomepageSource, string> = {
  frontmatter: 'localSkills.homepageFromAuthor',
  metadata: 'localSkills.homepageFromMetadata',
  registry: 'localSkills.homepageFromRegistry',
}

/** A skill's display name: its coordinate when known, else the directory name. */
function displayName(skill: LocalSkill): string {
  return skill.namespace ? `@${skill.namespace}/${skill.slug}` : skill.slug
}

/**
 * One locally installed skill: identity, where it lives, and the actions that
 * make sense for it. Actions are gated on `origin` — only a skill skillhub
 * installed can be updated, because only then do we know where it came from.
 */
export function LocalSkillCard({
  skill,
  busy,
  attachTargetCount,
  onCopyPath,
  onOpenFolder,
  onOpenHomepage,
  onUninstall,
  onUpdate,
  onAttach,
}: LocalSkillCardProps) {
  const { t } = useTranslation()

  const managed = skill.origin === 'managed'
  // Nothing resolvable behind any of the locations: a dangling or foreign link.
  const broken = !skill.realPath
  // Locations that are a link to a real directory. These are the ones whose
  // writes land elsewhere, so the count is worth surfacing on the title.
  const symlinkCount = skill.locations.filter((location) => location.kind === 'symlink').length
  const path = skill.realPath ?? skill.locations[0]?.path ?? ''

  return (
    <Card className={cn('p-4', broken && 'border-dashed')}>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate font-semibold text-foreground">{displayName(skill)}</h3>

            {/* Leads the badges: a symlinked skill is updated and removed at its
                target rather than here, which changes what every action does. */}
            {symlinkCount > 0 && (
              <span
                data-testid={`local-symlink-badge-${skill.slug}`}
                title={t('localSkills.symlinkTooltip', {
                  path: skill.realPath ?? '',
                })}
                className="inline-flex shrink-0 items-center gap-1 rounded-full bg-sky-500/15 px-2 py-0.5 text-[10px] font-semibold text-sky-600 ring-1 ring-inset ring-sky-500/30 dark:text-sky-400"
              >
                <Link2 className="h-3 w-3" />
                {symlinkCount > 1
                  ? t('localSkills.symlinkBadgeCount', { count: symlinkCount })
                  : t('localSkills.kindSymlink')}
              </span>
            )}

            <span
              className={cn(
                'shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium',
                managed
                  ? 'bg-primary/12 text-primary'
                  : 'bg-muted text-muted-foreground',
              )}
            >
              {managed ? t('localSkills.originManaged') : t('localSkills.originUnmanaged')}
            </span>

            {broken && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-destructive/10 px-2 py-0.5 text-[10px] font-medium text-destructive">
                <Link2Off className="h-3 w-3" />
                {t('localSkills.brokenLink')}
              </span>
            )}

            {skill.unreadable && (
              <span className="shrink-0 rounded-full bg-amber-500/15 px-2 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400">
                {t('localSkills.unreadable')}
              </span>
            )}

            {skill.homepage && (
              <button
                type="button"
                data-testid={`local-homepage-${skill.slug}`}
                aria-label={t('localSkills.openHomepage')}
                title={
                  skill.homepageSource
                    ? t(HOMEPAGE_SOURCE_LABEL[skill.homepageSource])
                    : t('localSkills.openHomepage')
                }
                onClick={() => onOpenHomepage(skill.homepage as string)}
                className="shrink-0 rounded-full p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <ExternalLink className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          {skill.metadataSlug && (
            <p className="mt-1 text-xs text-amber-600 dark:text-amber-400">
              {t('localSkills.metadataSlugMismatch', { slug: skill.metadataSlug })}
            </p>
          )}

          {/* Where the skill is reachable from. One chip per agent root; a
              single real directory shared by several agents shows as several
              chips on one card rather than several cards. */}
          <div className="mt-2 flex flex-wrap items-center gap-2">
            {skill.locations.map((location) => {
              const kindLabel = KIND_LABEL[location.kind]
              return (
                <span
                  key={`${location.agent}:${location.path}`}
                  title={location.path}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-border/60 bg-muted/40 px-2 py-1"
                >
                  <AgentBrandIcon id={location.agent} size={14} />
                  <span className="text-xs font-medium text-foreground">{location.agent}</span>
                  {kindLabel && (
                    <span className="text-[10px] text-muted-foreground">{t(kindLabel)}</span>
                  )}
                </span>
              )
            })}
          </div>

          {path && (
            <p className="mt-2 truncate font-mono text-xs text-muted-foreground">{path}</p>
          )}

          {skill.version && (
            <p className="mt-1 text-xs text-muted-foreground">
              {t('localSkills.version', { version: skill.version })}
            </p>
          )}
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
          {path && (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t('localSkills.copyPath')}
              title={t('localSkills.copyPath')}
              onClick={() => onCopyPath(path)}
            >
              <Copy className="h-4 w-4" />
            </Button>
          )}
          {skill.realPath && (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              data-testid={`local-open-${skill.slug}`}
              aria-label={t('localSkills.openInFolder')}
              title={t('localSkills.openInFolder')}
              disabled={busy}
              onClick={() => onOpenFolder(skill.realPath as string)}
            >
              <FolderOpen className="h-4 w-4" />
            </Button>
          )}
          {/* Only meaningful when the content is resolvable (`!broken`) and some
              other agent could still receive it. */}
          {!broken && attachTargetCount > 0 && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              data-testid={`local-attach-${skill.slug}`}
              disabled={busy}
              title={t('localSkills.attachTooltip')}
              onClick={() => onAttach(skill)}
            >
              <Plus className="h-4 w-4" />
              {t('localSkills.attach')}
            </Button>
          )}
          <Button
            type="button"
            variant="outline"
            size="sm"
            data-testid={`local-uninstall-${skill.slug}`}
            disabled={busy}
            onClick={() => onUninstall(skill)}
          >
            <Trash2 className="h-4 w-4" />
            {broken ? t('localSkills.cleanupLink') : t('localSkills.uninstall')}
          </Button>
          {/* Update needs a source to update from, which only a managed install
              has; an unmanaged skill gets the reason as a tooltip instead. */}
          {managed ? (
            <Button
              type="button"
              size="sm"
              data-testid={`local-update-${skill.slug}`}
              disabled={busy}
              onClick={() => onUpdate(skill)}
            >
              <RefreshCw className={busy ? 'h-4 w-4 animate-spin' : 'h-4 w-4'} />
              {t('localSkills.update')}
            </Button>
          ) : (
            <span
              className="cursor-help text-[11px] text-muted-foreground"
              title={t('localSkills.noUpdateUnmanaged')}
            >
              {t('localSkills.noUpdateUnmanagedShort')}
            </span>
          )}
        </div>
      </div>
    </Card>
  )
}
