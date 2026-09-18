import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, FolderOpen, RotateCcw, SlidersHorizontal } from 'lucide-react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
  Dialog,
  DialogContent,
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
import { Input } from '@/shared/ui/input'
import { cn } from '@/shared/lib/utils'
import { useInstallMode } from '@/shared/hooks/use-install-mode'
import type { InstallMode } from '@/shared/lib/install-mode'
import {
  getSkillStoragePath,
  resetSkillStoragePath,
  setSkillStoragePath,
} from '@/features/skill/tauri-installer'

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

        {/* A plain caption, not a control. The boxed style above read as another
            config item; this is a footnote the reader should glance past. */}
        <p className="mt-3 text-xs leading-relaxed text-muted-foreground/80">
          {t('settings.installMode.note')}
        </p>
      </SettingsSection>

      {/* Its own section, visually separated from install-mode: same page, but a
          distinct feature. The divider and its own title make that explicit. */}
      <SettingsSection
        title={t('settings.storagePath.label')}
        description={t('settings.storagePath.hint')}
      >
        <StoragePathControl />
      </SettingsSection>
    </div>
  )
}

/**
 * The skill repository path control.
 *
 * A read-only input shows where the shared repository lives; the folder button
 * relocates it. Relocation is destructive — it moves every skill and re-points
 * every agent symlink — so it is preceded by a confirm dialog, then the native
 * directory picker. `pendingAction` holds which action the confirm dialog is
 * about to run, so a single dialog serves both "move to a new directory" and
 * "restore the default".
 */
type StorageAction = 'pick' | 'reset'

function StoragePathControl() {
  const { t } = useTranslation()
  const [currentPath, setCurrentPath] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [warnings, setWarnings] = useState<string[]>([])
  const [pendingAction, setPendingAction] = useState<StorageAction | null>(null)

  useEffect(() => {
    let active = true
    getSkillStoragePath().then((value) => {
      if (active && value) setCurrentPath(value)
    })
    return () => {
      active = false
    }
  }, [])

  const runMigration = async (action: StorageAction, target?: string) => {
    setBusy(true)
    setError(null)
    setWarnings([])
    try {
      const result =
        action === 'reset'
          ? await resetSkillStoragePath()
          : target
            ? await setSkillStoragePath(target)
            : null
      if (!result) {
        setError(t('settings.storagePath.error'))
        return
      }
      if (result.newPath) setCurrentPath(result.newPath)
      setWarnings(result.warnings ?? [])
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.storagePath.error'))
    } finally {
      setBusy(false)
    }
  }

  const handleConfirm = async () => {
    const action = pendingAction
    setPendingAction(null)
    if (!action) return
    if (action === 'reset') {
      await runMigration('reset')
      return
    }
    // The user already confirmed above; now the native directory picker opens,
    // and only a real selection triggers the move. A failure here (e.g. the
    // dialog permission is missing) must surface, not silently swallow.
    try {
      const selected = await openDialog({ directory: true })
      if (typeof selected !== 'string' || !selected) return
      await runMigration('pick', selected)
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.storagePath.error'))
    }
  }

  const disabled = busy || !currentPath

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <Input
          readOnly
          value={currentPath}
          placeholder={t('settings.storagePath.placeholder')}
          aria-label={t('settings.storagePath.label')}
          data-testid="skill-storage-path-input"
        />
        <Button
          variant="outline"
          size="icon"
          disabled={disabled}
          aria-label={t('settings.storagePath.pick')}
          data-testid="skill-storage-path-pick"
          onClick={() => setPendingAction('pick')}
        >
          <FolderOpen className="h-4 w-4" aria-hidden="true" />
        </Button>
      </div>

      <Button
        variant="ghost"
        size="sm"
        disabled={disabled}
        data-testid="skill-storage-path-reset"
        onClick={() => setPendingAction('reset')}
      >
        <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
        {t('settings.storagePath.reset')}
      </Button>

      {busy ? (
        <p className="text-xs text-muted-foreground">{t('settings.storagePath.loading')}</p>
      ) : null}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {warnings.length > 0 ? (
        <ul className="space-y-1 text-xs text-muted-foreground">
          {warnings.map((warning, index) => (
            <li key={index}>{warning}</li>
          ))}
        </ul>
      ) : null}

      <AlertDialog
        open={pendingAction !== null}
        onOpenChange={(open) => {
          if (!open) setPendingAction(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {pendingAction === 'reset'
                ? t('settings.storagePath.resetConfirmTitle')
                : t('settings.storagePath.confirmTitle')}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {pendingAction === 'reset'
                ? t('settings.storagePath.resetConfirmBody')
                : t('settings.storagePath.confirmBody')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button
              variant="ghost"
              data-testid="storage-confirm-cancel"
              onClick={() => setPendingAction(null)}
            >
              {t('settings.storagePath.cancel')}
            </Button>
            <Button variant="default" data-testid="storage-confirm-ok" onClick={handleConfirm}>
              {t('settings.storagePath.confirm')}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
