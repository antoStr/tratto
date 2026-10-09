import { forwardRef, useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Dialog as RDialog, DropdownMenu, Switch as RSwitch, Tooltip as RTooltip } from 'radix-ui'
import { Check, X } from 'lucide-react'
import { create } from 'zustand'

/* ---------- Tooltip ---------- */

export function Tip({ label, kbd, side = 'top', children }: { label: string; kbd?: string; side?: 'top' | 'bottom' | 'left' | 'right'; children: ReactNode }) {
  return (
    <RTooltip.Root>
      <RTooltip.Trigger asChild>{children}</RTooltip.Trigger>
      <RTooltip.Portal>
        <RTooltip.Content className="tooltip" side={side} sideOffset={8}>
          {label}
          {kbd && <kbd>{kbd}</kbd>}
        </RTooltip.Content>
      </RTooltip.Portal>
    </RTooltip.Root>
  )
}

/* ---------- Icon button (always labelled) ---------- */

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string
  kbd?: string
  active?: boolean
  on?: boolean
  size?: 'sm'
  tipSide?: 'top' | 'bottom' | 'left' | 'right'
  noTip?: boolean
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, kbd, active, on, size, tipSide, noTip, className, children, ...rest },
  ref,
) {
  const btn = (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      aria-pressed={active ?? on}
      data-active={active || undefined}
      data-on={on || undefined}
      className={`icon-btn ${size ?? ''} ${className ?? ''}`}
      {...rest}
    >
      {children}
    </button>
  )
  return noTip ? btn : <Tip label={label} kbd={kbd} side={tipSide}>{btn}</Tip>
})

/* ---------- Menu ---------- */

export const Menu = DropdownMenu.Root
export const MenuTrigger = DropdownMenu.Trigger

export function MenuContent({ children, align = 'start', side = 'bottom' }: { children: ReactNode; align?: 'start' | 'center' | 'end'; side?: 'top' | 'bottom' | 'left' | 'right' }) {
  return (
    <DropdownMenu.Portal>
      <DropdownMenu.Content className="menu" align={align} side={side} sideOffset={6} collisionPadding={8}>
        {children}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  )
}

export function MenuItem({ icon, kbd, danger, children, onSelect, disabled }: { icon?: ReactNode; kbd?: string; danger?: boolean; children: ReactNode; onSelect?: () => void; disabled?: boolean }) {
  return (
    <DropdownMenu.Item className={`menu-item ${danger ? 'danger' : ''}`} onSelect={onSelect} disabled={disabled}>
      <span className="menu-icon" aria-hidden="true">
        {icon}
      </span>
      <span>{children}</span>
      {kbd && <span className="menu-kbd">{kbd}</span>}
    </DropdownMenu.Item>
  )
}

export function MenuCheck({ checked, children, onSelect }: { checked: boolean; children: ReactNode; onSelect: () => void }) {
  return (
    <DropdownMenu.CheckboxItem className="menu-item" checked={checked} onSelect={(e) => (e.preventDefault(), onSelect())}>
      <span className="menu-icon" aria-hidden="true">
        {checked && <Check size={14} />}
      </span>
      <span>{children}</span>
    </DropdownMenu.CheckboxItem>
  )
}

export function MenuSub({ label, icon, children }: { label: string; icon?: ReactNode; children: ReactNode }) {
  return (
    <DropdownMenu.Sub>
      <DropdownMenu.SubTrigger className="menu-item">
        <span className="menu-icon" aria-hidden="true">
          {icon}
        </span>
        <span>{label}</span>
        <span className="menu-kbd" aria-hidden="true">
          ›
        </span>
      </DropdownMenu.SubTrigger>
      <DropdownMenu.Portal>
        <DropdownMenu.SubContent className="menu" sideOffset={8} collisionPadding={8}>
          {children}
        </DropdownMenu.SubContent>
      </DropdownMenu.Portal>
    </DropdownMenu.Sub>
  )
}

/** A menu that opens at a point on screen (client coordinates), for right clicks. */
export function MenuAt({ at, onClose, children }: { at: { x: number; y: number } | null; onClose: () => void; children: ReactNode }) {
  return (
    <DropdownMenu.Root open={!!at} onOpenChange={(o) => !o && onClose()} modal={false}>
      <DropdownMenu.Trigger asChild>
        <span className="menu-anchor" aria-hidden="true" style={{ left: at?.x ?? 0, top: at?.y ?? 0 }} />
      </DropdownMenu.Trigger>
      {at && children}
    </DropdownMenu.Root>
  )
}

export const MenuSep = () => <DropdownMenu.Separator className="menu-sep" />
export const MenuLabel = ({ children }: { children: ReactNode }) => <DropdownMenu.Label className="menu-label">{children}</DropdownMenu.Label>

/* ---------- Dialog ---------- */

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  wide,
  children,
  footer,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description?: ReactNode
  wide?: boolean
  children?: ReactNode
  footer?: ReactNode
}) {
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="dialog-overlay" />
        <RDialog.Content className={`dialog ${wide ? 'wide' : ''}`} aria-describedby={description ? undefined : undefined}>
          <div className="dialog-header">
            <RDialog.Title className="dialog-title">{title}</RDialog.Title>
            <RDialog.Close asChild>
              <IconButton label="Chiudi" noTip>
                <X size={16} />
              </IconButton>
            </RDialog.Close>
          </div>
          <div className="dialog-body">
            {description ? <RDialog.Description className="dialog-desc">{description}</RDialog.Description> : <RDialog.Description className="sr-only">{title}</RDialog.Description>}
            {children}
          </div>
          {footer && <div className="dialog-footer">{footer}</div>}
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  )
}

/* ---------- Switch ---------- */

export function Switch({ checked, onChange, label, id }: { checked: boolean; onChange: (v: boolean) => void; label: string; id: string }) {
  return (
    <div className="switch-row">
      <label htmlFor={id}>{label}</label>
      <RSwitch.Root id={id} className="switch" checked={checked} onCheckedChange={onChange}>
        <RSwitch.Thumb className="switch-thumb" />
      </RSwitch.Root>
    </div>
  )
}

/* ---------- Segmented (radio group) ---------- */

export function Segmented<T extends string>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; title?: string }[]; label: string }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          aria-label={o.title}
          title={o.title}
          tabIndex={o.value === value ? 0 : -1}
          onClick={() => onChange(o.value)}
          onKeyDown={(e) => {
            const i = options.findIndex((x) => x.value === value)
            if (e.key === 'ArrowRight' || e.key === 'ArrowDown') onChange(options[(i + 1) % options.length].value)
            if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') onChange(options[(i - 1 + options.length) % options.length].value)
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

/* ---------- Colour swatches ---------- */

export function Swatches({ colors, value, onChange, label, allowTransparent, allowCustom = true }: { colors: string[]; value: string; onChange: (c: string) => void; label: string; allowTransparent?: boolean; allowCustom?: boolean }) {
  const custom = /^#[0-9a-f]{6}$/i.test(value) && !colors.some((c) => c.toLowerCase() === value.toLowerCase())
  return (
    <div className="swatches" role="radiogroup" aria-label={label}>
      {allowTransparent && <button type="button" role="radio" aria-checked={value === 'transparent'} aria-label="Nessun colore" title="Nessun colore" className="swatch transparent" onClick={() => onChange('transparent')} />}
      {colors.map((c) => (
        <button key={c} type="button" role="radio" aria-checked={c.toLowerCase() === value.toLowerCase()} aria-label={c} title={c} className="swatch" style={{ background: c }} onClick={() => onChange(c)} />
      ))}
      {allowCustom && (
        <label className="swatch custom" data-checked={custom} title="Colore personalizzato">
          <span className="sr-only">Colore personalizzato</span>
          <input type="color" value={/^#[0-9a-f]{6}$/i.test(value) ? value : '#000000'} onChange={(e) => onChange(e.target.value.toUpperCase())} />
        </label>
      )}
    </div>
  )
}

/* ---------- Number field with draggable label (Figma scrubbing) ---------- */

export function NumberField({ label, value, onChange, step = 1, min = -Infinity, max = Infinity, suffix, title, decimals = 0 }: { label: ReactNode; value: number | null; onChange: (v: number) => void; step?: number; min?: number; max?: number; suffix?: string; title: string; decimals?: number }) {
  const [draft, setDraft] = useState<string | null>(null)
  const drag = useRef<{ x: number; v: number } | null>(null)
  const shown = value === null ? 'Misto' : `${Number(value.toFixed(decimals))}${suffix ?? ''}`
  const commit = (text: string) => {
    setDraft(null)
    const n = Number.parseFloat(text.replace(',', '.'))
    if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)))
  }
  return (
    <div className="field" title={title}>
      <span
        className="field-label"
        aria-hidden="true"
        onPointerDown={(e) => {
          if (value === null) return
          ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
          drag.current = { x: e.clientX, v: value }
        }}
        onPointerMove={(e) => {
          if (!drag.current) return
          const d = Math.round((e.clientX - drag.current.x) / 2) * step * (e.shiftKey ? 10 : 1)
          onChange(Math.min(max, Math.max(min, drag.current.v + d)))
        }}
        onPointerUp={() => (drag.current = null)}
      >
        {label}
      </span>
      <input
        aria-label={title}
        className="num"
        value={draft ?? shown}
        onFocus={(e) => {
          setDraft(value === null ? '' : String(Number(value.toFixed(decimals))))
          requestAnimationFrame(() => e.target.select())
        }}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={(e) => commit(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
          if (e.key === 'Escape') {
            setDraft(null)
            ;(e.target as HTMLInputElement).blur()
          }
          if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && value !== null) {
            e.preventDefault()
            const d = (e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1)
            const next = Math.min(max, Math.max(min, value + d))
            onChange(next)
            setDraft(String(Number(next.toFixed(decimals))))
          }
        }}
      />
    </div>
  )
}

/* ---------- Toasts ---------- */

interface Toast {
  id: number
  text: string
  kind: 'info' | 'error'
  action?: { label: string; run: () => void }
}
const useToasts = create<{ list: Toast[] }>(() => ({ list: [] }))
let toastId = 0

export function toast(text: string, kind: Toast['kind'] = 'info', action?: Toast['action']) {
  const id = ++toastId
  useToasts.setState((s) => ({ list: [...s.list.slice(-2), { id, text, kind, action }] }))
  setTimeout(() => useToasts.setState((s) => ({ list: s.list.filter((t) => t.id !== id) })), kind === 'error' ? 6000 : 3200)
}

export function Toaster() {
  const list = useToasts((s) => s.list)
  return (
    <div className="toasts" role="status" aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          <span>{t.text}</span>
          {t.action && (
            <button type="button" onClick={t.action.run}>
              {t.action.label}
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

/* ---------- Misc ---------- */

export function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (!parts.length) return '?'
  return (parts[0][0] + (parts[1]?.[0] ?? '')).toUpperCase()
}

export function Avatar({ name, color, size }: { name: string; color: string; size?: 'lg' }) {
  return (
    <span className={`avatar ${size ?? ''}`} style={{ background: color }} aria-hidden="true">
      {initials(name)}
    </span>
  )
}

/** Re-renders on an interval, for relative times. */
export function useTick(ms: number) {
  const [, setN] = useState(0)
  useEffect(() => {
    const t = setInterval(() => setN((n) => n + 1), ms)
    return () => clearInterval(t)
  }, [ms])
}

export function Logo({ size = 20 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 512 512" aria-hidden="true">
      <rect width="512" height="512" rx="116" fill="#1E1E1E" />
      <path d="M112 360C164 262 226 176 300 152c62-20 104 8 96 56-8 46-74 78-130 112-32 20-46 40-36 54 14 18 72 6 142-40l10 14c-80 62-160 82-188 44-20-28 2-62 50-92 54-34 112-58 118-92 6-32-24-42-62-30-60 20-112 92-164 184Z" fill="#FFFFFF" />
      <circle cx="394" cy="330" r="24" fill="#0D99FF" />
    </svg>
  )
}
