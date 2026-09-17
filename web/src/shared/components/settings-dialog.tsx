import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, SlidersHorizontal } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/shared/ui/dialog'
import { cn } from '@/shared/lib/utils'
import { useInstallMode } from '@/shared/hooks/use-install-mode'
import type { InstallMode } from '@/shared/lib/install-mode'

/**
 * Left-hand menu entries.
 *
 * An array rather than inline JSX so a second pane is a data change and the
 * selection / active-styling logic never has to be rewritten per item.
 */
const SETTINGS_MENUS = [
  { key: 'skill', icon: SlidersHorizontal },
] as const

type SettingsMenuKey = (typeof SETTINGS_MENUS)[number]['key']

interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * Centered settings modal, rendered by the app shell (outside the header).
 *
 * Uses the app-standard `Dialog`, which supplies the overlay, centering, focus
 * trap and Escape handling. It must not be declared inside the header: the header
 * has `backdrop-blur`, which makes it a containing block for `position: fixed`
 * descendants, so the modal would center on the header box instead of the
 * viewport.
 *
 * No trigger here — see `SettingsTrigger`. The two are separate because they live
 * in different parts of the shell tree.
 */
export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const { t } = useTranslation()
  const [activeMenu, setActiveMenu] = useState<SettingsMenuKey>('skill')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(calc(100vw-2rem),46rem)] gap-0 overflow-hidden p-0">
        <div className="flex min-h-[22rem]">
          {/* Left: menu list. Selection is carried by weight, colour and a left
              accent bar together — colour alone was too easy to miss. */}
          <nav
            aria-label={t('settings.title')}
            className="w-48 shrink-0 border-r border-border/60 bg-muted/25 p-3"
          >
            <DialogTitle className="px-3 pb-3 pt-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/70">
              {t('settings.title')}
            </DialogTitle>
            {SETTINGS_MENUS.map(({ key, icon: Icon }) => {
              const active = activeMenu === key
              return (
                <button
                  key={key}
                  type="button"
                  data-testid={`settings-menu-${key}`}
                  aria-current={active ? 'page' : undefined}
                  onClick={() => setActiveMenu(key)}
                  className={cn(
                    'mb-1 flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                    // Filled rather than tinted: `--primary` is near-black in the
                    // light theme, so `text-primary` reads as ordinary body text
                    // and `bg-primary/12` is a barely-there grey. A solid pill is
                    // the only treatment here that is unambiguous — and it matches
                    // how the header marks the active nav item.
                    active
                      ? 'bg-primary font-semibold text-primary-foreground shadow-[0_1px_2px_0_rgb(0_0_0/0.12)]'
                      : 'font-medium text-muted-foreground hover:bg-background/70 hover:text-foreground',
                  )}
                >
                  <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
                  <span className="truncate">{t(`settings.menus.${key}`)}</span>
                </button>
              )
            })}
          </nav>

          {/* Right: the selected menu's settings. `pr-14` clears the dialog's
              absolutely-positioned close button. */}
          <div className="min-w-0 flex-1 overflow-y-auto p-6 pr-14">
            {activeMenu === 'skill' ? <SkillSettings /> : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

/**
 * One grouped block of settings.
 *
 * Everything in the right pane is wrapped in one of these so that adding a second
 * setting is a matter of adding another `<SettingsSection>`: the title, the
 * description and the divider between blocks come with it, instead of each new
 * setting inventing its own spacing and heading level.
 */
function SettingsSection({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: ReactNode
}) {
  return (
    <section className="border-b border-border/50 pb-6 last:border-b-0 last:pb-0">
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      {description ? (
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
      ) : null}
      <div className="mt-3.5">{children}</div>
    </section>
  )
}

/** The "Skill 配置" pane. */
function SkillSettings() {
  const { t } = useTranslation()
  const { mode, setMode } = useInstallMode()

  const options: Array<{
    mode: InstallMode
    name: string
    hint: string
    desc: string
  }> = [
    {
      mode: 'shared',
      name: t('settings.installMode.shared.name'),
      hint: t('settings.installMode.shared.hint'),
      desc: t('settings.installMode.shared.desc'),
    },
    {
      mode: 'copy',
      name: t('settings.installMode.copy.name'),
      hint: t('settings.installMode.copy.hint'),
      desc: t('settings.installMode.copy.desc'),
    },
  ]

  return (
    <div className="space-y-6">
      <SettingsSection
        title={t('settings.installMode.label')}
        description={t('settings.installMode.subtitle')}
      >
        {/* A single-choice group, so radio semantics rather than a switch: the
            selected option is already visible in the list, and a separate switch
            would give the same state two controls that can disagree. */}
        <div
          role="radiogroup"
          aria-label={t('settings.installMode.label')}
          className="space-y-2"
        >
          {options.map((option) => {
            const active = option.mode === mode
            return (
              <button
                key={option.mode}
                type="button"
                role="radio"
                aria-checked={active}
                data-testid={`install-mode-option-${option.mode}`}
                onClick={() => setMode(option.mode)}
                className={cn(
                  'flex w-full items-start gap-3 rounded-xl border p-3.5 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  active
                    ? 'border-primary bg-primary/5'
                    : 'border-border/60 bg-muted/20 hover:bg-muted/50',
                )}
              >
                <span
                  aria-hidden="true"
                  className={cn(
                    'mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors',
                    active ? 'border-primary bg-primary' : 'border-muted-foreground/40',
                  )}
                >
                  {active ? (
                    <Check className="h-3 w-3 text-primary-foreground" strokeWidth={3} />
                  ) : null}
                </span>
                <span className="min-w-0">
                  <span className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                    <span className="text-sm font-semibold text-foreground">
                      {option.name}
                    </span>
                    <span className="text-[11px] text-muted-foreground">
                      {option.hint}
                    </span>
                  </span>
                  <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
                    {option.desc}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      </SettingsSection>

      {/* Its own block, not a footnote inside the one above: this is what stops
          the user from worrying that switching will move existing skills. */}
      <SettingsSection title={t('settings.installMode.noteTitle')}>
        <DialogDescription className="rounded-lg bg-muted/50 p-3 text-xs leading-relaxed text-muted-foreground">
          {t('settings.installMode.note')}
        </DialogDescription>
      </SettingsSection>
    </div>
  )
}
