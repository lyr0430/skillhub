import { createPortal } from 'react-dom'
import { AlertTriangle, CheckCircle2, Info, Loader2, X, XCircle } from 'lucide-react'
import { Toaster as Sonner } from 'sonner'
import { getPortalContainer } from '@/shared/lib/portal-container'
import { CENTER_TOASTER_ID } from '@/shared/lib/toast'

const ICON_CHIP =
  'flex h-6 w-6 shrink-0 items-center justify-center rounded-full [&>svg]:h-3.5 [&>svg]:w-3.5'

export function Toaster() {
  const node = (
    <div translate="no">
      <Sonner
        id={CENTER_TOASTER_ID}
        position="top-center"
        className="!left-1/2 !right-auto !top-4 !-translate-x-1/2 !z-[100]"
        offset={16}
        mobileOffset={16}
        closeButton
        icons={{
          success: (
            <span className={`${ICON_CHIP} bg-emerald-500/15 text-emerald-600 dark:text-emerald-400`}>
              <CheckCircle2 />
            </span>
          ),
          error: (
            <span className={`${ICON_CHIP} bg-destructive/15 text-destructive`}>
              <XCircle />
            </span>
          ),
          warning: (
            <span className={`${ICON_CHIP} bg-amber-500/15 text-amber-600 dark:text-amber-400`}>
              <AlertTriangle />
            </span>
          ),
          info: (
            <span className={`${ICON_CHIP} bg-blue-500/15 text-blue-600 dark:text-blue-400`}>
              <Info />
            </span>
          ),
          loading: (
            <span className={`${ICON_CHIP} bg-muted text-muted-foreground`}>
              <Loader2 className="animate-spin" />
            </span>
          ),
          close: <X className="h-3.5 w-3.5" />,
        }}
        toastOptions={{
          toasterId: CENTER_TOASTER_ID,
          classNames: {
            toast: 'glass-strong mx-auto w-fit max-w-[min(100vw-2rem,32rem)] border border-border/40',
            title: 'text-foreground font-semibold text-center',
            description: 'text-muted-foreground text-center',
            content: 'w-full text-center',
            actionButton: 'bg-primary text-primary-foreground',
            cancelButton: 'bg-muted text-muted-foreground',
            error: 'border-destructive/35',
            success: 'border-emerald-500/35',
            warning: 'border-amber-500/35',
            info: 'border-blue-500/35',
          },
        }}
      />
    </div>
  )

  const host = typeof document !== 'undefined' ? getPortalContainer() : undefined
  if (host) {
    return createPortal(node, host)
  }
  return node
}
