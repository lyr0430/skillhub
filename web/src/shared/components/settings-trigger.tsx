import { useTranslation } from 'react-i18next'
import { Settings } from 'lucide-react'
import { isTauri } from '@/shared/lib/tauri'
import { cn } from '@/shared/lib/utils'

interface SettingsTriggerProps {
  open: boolean
  onToggle: () => void
  className?: string
}

/**
 * Desktop-only settings entry: a gear beside the theme toggle.
 *
 * Renders nothing outside the Tauri shell — in a browser the entry does not
 * exist at all (no trigger, no route).
 *
 * The trigger and the dialog are deliberately separate components rendered in
 * different parts of the shell. That is not cosmetic: the header carries
 * `backdrop-blur`, and a non-`none` `backdrop-filter` makes an element a
 * containing block for its `position: fixed` descendants. A centered modal
 * declared inside the header would therefore be centered on the *header box*
 * rather than the viewport. `Layout` owns the open state and renders the dialog
 * outside the header; this component never renders one itself.
 */
export function SettingsTrigger({ open, onToggle, className }: SettingsTriggerProps) {
  const { t } = useTranslation()

  if (!isTauri()) {
    return null
  }

  return (
    <button
      type="button"
      data-testid="settings-trigger"
      aria-label={t('settings.open')}
      aria-haspopup="dialog"
      aria-expanded={open}
      title={t('settings.open')}
      onClick={onToggle}
      className={cn(
        'inline-flex items-center justify-center rounded-lg p-2 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        open
          ? 'bg-accent text-foreground'
          : 'text-muted-foreground hover:bg-accent hover:text-foreground',
        className,
      )}
    >
      <Settings className="h-4 w-4" />
    </button>
  )
}
