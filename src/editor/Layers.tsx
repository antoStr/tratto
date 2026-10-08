import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ArrowLeft,
  ArrowUpRight,
  ChevronDown,
  Circle,
  Download,
  Eye,
  EyeOff,
  Highlighter,
  Image as ImageIcon,
  ImagePlus,
  Keyboard,
  Lock,
  Minus,
  Moon,
  PanelsTopLeft,
  PenLine,
  Plus,
  Settings2,
  Smile,
  Square,
  StickyNote,
  Type,
  Unlock,
} from 'lucide-react'
import type { Board } from './doc.ts'
import { commands } from './controller.ts'
import { aabb, union } from './geometry.ts'
import { elementLabel } from './Inspector.tsx'
import { useBoardVersion } from './overlays.tsx'
import { drawElement } from './render.ts'
import { useEditor, type Prefs } from './store.ts'
import { TEMPLATES, type Template } from './templates.ts'
import type { El } from './types.ts'
import { IconButton, Logo, Menu, MenuCheck, MenuContent, MenuItem, MenuSep, MenuSub, MenuTrigger } from '../ui.tsx'

const ROW = 32

export type Status = 'connecting' | 'synced' | 'offline'

interface Props {
  board: Board
  title: string
  host: boolean
  status: Status
  onRename?: (title: string) => void
  onHome?: () => void
  onNewBoard?: () => void
  onExport: () => void
  onShortcuts: () => void
  onInsertImage: () => void
}

export function LeftPanel(p: Props) {
  const [tab, setTab] = useState<'layers' | 'templates'>('layers')
  const readOnly = useEditor((s) => s.readOnly)
  return (
    <aside className="panel panel-left" aria-label="Livelli">
      <FileHead {...p} />
      <div className="tabs" role="tablist" aria-label="Pannello">
        <button type="button" role="tab" className="tab-text" aria-selected={tab === 'layers'} onClick={() => setTab('layers')}>
          Livelli
        </button>
        {!readOnly && (
          <button type="button" role="tab" className="tab-text" aria-selected={tab === 'templates'} onClick={() => setTab('templates')}>
            Modelli
          </button>
        )}
      </div>
      {tab === 'layers' || readOnly ? <LayerList board={p.board} /> : <TemplateList />}
    </aside>
  )
}

/* ---------- header: app menu, name, status ---------- */

const STATUS_TEXT: Record<Status, string> = { connecting: 'Connessione…', synced: 'Tutto salvato', offline: 'Non connesso' }

function FileHead({ title, host, status, onRename, onHome, onNewBoard, onExport, onShortcuts, onInsertImage }: Props) {
  const [editing, setEditing] = useState(false)
  const readOnly = useEditor((s) => s.readOnly)
  return (
    <div className="file-head">
      <AppMenu host={host} readOnly={readOnly} onHome={onHome} onNewBoard={onNewBoard} onExport={onExport} onShortcuts={onShortcuts} onInsertImage={onInsertImage} />
      <div className="file-name">
        {editing && onRename ? (
          <input
            className="rename-input"
            aria-label="Nome della lavagna"
            defaultValue={title}
            autoFocus
            onFocus={(e) => e.target.select()}
            onBlur={(e) => {
              setEditing(false)
              if (e.target.value.trim() && e.target.value.trim() !== title) onRename(e.target.value.trim())
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
              if (e.key === 'Escape') setEditing(false)
            }}
          />
        ) : (
          <button type="button" className="file-title" title={onRename ? 'Rinomina' : title} onClick={() => onRename && setEditing(true)} disabled={!onRename}>
            {title}
          </button>
        )}
        <div className="file-sub" role="status" aria-live="polite">
          <span className="status-dot" data-status={status} aria-hidden="true" />
          {host ? STATUS_TEXT[status] : status === 'synced' ? 'Connesso' : STATUS_TEXT[status]}
        </div>
      </div>
    </div>
  )
}

function AppMenu({ host, readOnly, onHome, onNewBoard, onExport, onShortcuts, onInsertImage }: { host: boolean; readOnly: boolean; onHome?: () => void; onNewBoard?: () => void; onExport: () => void; onShortcuts: () => void; onInsertImage: () => void }) {
  const prefs = useEditor((s) => s.prefs)
  const setPrefs = useEditor((s) => s.setPrefs)
  const toggle = (k: keyof Prefs) => setPrefs({ [k]: !prefs[k] } as Partial<Prefs>)
  return (
    <Menu>
      <MenuTrigger asChild>
        <button type="button" className="app-menu-btn" aria-label="Menu principale">
          <Logo size={20} />
          <ChevronDown size={12} />
        </button>
      </MenuTrigger>
      <MenuContent>
        {host && (
          <>
            <MenuItem icon={<ArrowLeft size={14} />} onSelect={onHome}>
              Torna alle lavagne
            </MenuItem>
            <MenuItem icon={<Plus size={14} />} onSelect={onNewBoard}>
              Nuova lavagna
            </MenuItem>
            <MenuSep />
          </>
        )}
        {!readOnly && (
          <MenuItem icon={<ImagePlus size={14} />} kbd="I" onSelect={onInsertImage}>
            Inserisci immagine…
          </MenuItem>
        )}
        <MenuItem icon={<Download size={14} />} kbd="Ctrl+Maiusc+E" onSelect={onExport}>
          Esporta…
        </MenuItem>
        <MenuSep />
        <MenuSub label="Tema" icon={<Moon size={14} />}>
          <MenuCheck checked={prefs.theme === 'system'} onSelect={() => setPrefs({ theme: 'system' })}>
            Come il sistema
          </MenuCheck>
          <MenuCheck checked={prefs.theme === 'light'} onSelect={() => setPrefs({ theme: 'light' })}>
            Chiaro
          </MenuCheck>
          <MenuCheck checked={prefs.theme === 'dark'} onSelect={() => setPrefs({ theme: 'dark' })}>
            Scuro
          </MenuCheck>
        </MenuSub>
        <MenuSub label="Preferenze" icon={<Settings2 size={14} />}>
          <MenuCheck checked={prefs.fingerDraw} onSelect={() => toggle('fingerDraw')}>
            Disegna con le dita
          </MenuCheck>
          <MenuCheck checked={prefs.wheel === 'zoom'} onSelect={() => setPrefs({ wheel: prefs.wheel === 'zoom' ? 'pan' : 'zoom' })}>
            La rotellina del mouse fa zoom
          </MenuCheck>
          <MenuCheck checked={prefs.inkToShape} onSelect={() => toggle('inkToShape')}>
            Da tratto a forma
          </MenuCheck>
        </MenuSub>
        <MenuItem icon={<PanelsTopLeft size={14} />} kbd="Ctrl+\" onSelect={() => useEditor.setState((s) => ({ ui: !s.ui }))}>
          Mostra o nascondi i pannelli
        </MenuItem>
        <MenuItem icon={<Keyboard size={14} />} kbd="?" onSelect={onShortcuts}>
          Scorciatoie da tastiera
        </MenuItem>
      </MenuContent>
    </Menu>
  )
}

/* ---------- layers ---------- */

function typeIcon(el: El) {
  switch (el.type) {
    case 'ink':
      return <PenLine size={14} />
    case 'highlighter':
      return <Highlighter size={14} />
    case 'shape':
      return el.shape === 'ellipse' ? <Circle size={14} /> : <Square size={14} />
    case 'line':
      return el.arrowEnd || el.arrowStart ? <ArrowUpRight size={14} /> : <Minus size={14} />
    case 'text':
      return <Type size={14} />
    case 'sticky':
      return <StickyNote size={14} />
    case 'image':
      return <ImageIcon size={14} />
    case 'stamp':
      return <Smile size={14} />
  }
}

function LayerList({ board }: { board: Board }) {
  useBoardVersion(board)
  const selection = useEditor((s) => s.selection)
  const readOnly = useEditor((s) => s.readOnly)
  const scroller = useRef<HTMLDivElement>(null)
  const [scroll, setScroll] = useState({ top: 0, height: 600 })
  const [renaming, setRenaming] = useState<string | null>(null)
  const anchor = useRef<string | null>(null)
  const items = board.all().slice().reverse()
  const selected = useMemo(() => new Set(selection), [selection])

  useEffect(() => {
    const el = scroller.current!
    const ro = new ResizeObserver(() => setScroll((s) => ({ ...s, height: el.clientHeight })))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Bring the selection into view when it changes on the canvas.
  useEffect(() => {
    const el = scroller.current
    if (!el || !selection.length) return
    const i = items.findIndex((it) => it.id === selection[selection.length - 1])
    if (i < 0) return
    const top = i * ROW
    if (top < el.scrollTop || top + ROW > el.scrollTop + el.clientHeight) el.scrollTop = top - el.clientHeight / 2
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection])

  const first = Math.max(0, Math.floor(scroll.top / ROW) - 5)
  const last = Math.min(items.length, Math.ceil((scroll.top + scroll.height) / ROW) + 5)

  const click = (e: React.MouseEvent, el: El) => {
    const set = useEditor.setState
    if (e.shiftKey && anchor.current) {
      const a = items.findIndex((it) => it.id === anchor.current)
      const b = items.findIndex((it) => it.id === el.id)
      const [from, to] = a < b ? [a, b] : [b, a]
      set({ selection: items.slice(from, to + 1).map((it) => it.id), tool: 'select' })
      return
    }
    anchor.current = el.id
    if (e.ctrlKey || e.metaKey) set({ selection: selected.has(el.id) ? selection.filter((id) => id !== el.id) : [...selection, el.id], tool: 'select' })
    else set({ selection: [el.id], tool: 'select' })
  }

  return (
    <div className="panel-scroll" ref={scroller} onScroll={(e) => setScroll({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}>
      {!items.length ? (
        <p className="layers-empty">Ancora niente qui. Tutto quello che disegni o aggiungi comparirà in questo elenco.</p>
      ) : (
        <div className="layers" role="listbox" aria-label="Livelli" aria-multiselectable="true" style={{ height: items.length * ROW + 12 }}>
          {items.slice(first, last).map((el, k) => {
            const i = first + k
            return (
              <div
                key={el.id}
                role="option"
                aria-selected={selected.has(el.id)}
                className="layer-row"
                data-dim={el.hidden || undefined}
                style={{ top: 4 + i * ROW }}
                onClick={(e) => click(e, el)}
                onDoubleClick={() => !readOnly && setRenaming(el.id)}
              >
                <span className="layer-icon" aria-hidden="true">
                  {typeIcon(el)}
                </span>
                {renaming === el.id ? (
                  <input
                    className="rename-input"
                    aria-label="Nome del livello"
                    defaultValue={elementLabel(el)}
                    autoFocus
                    onFocus={(e) => e.target.select()}
                    onClick={(e) => e.stopPropagation()}
                    onBlur={(e) => {
                      setRenaming(null)
                      board.update(el.id, { name: e.target.value.trim().slice(0, 80) || undefined })
                    }}
                    onKeyDown={(e) => {
                      e.stopPropagation()
                      if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                      if (e.key === 'Escape') setRenaming(null)
                    }}
                  />
                ) : (
                  <span className="layer-name">{elementLabel(el)}</span>
                )}
                {!readOnly && (
                  <span className="layer-actions" data-pinned={el.locked || el.hidden || undefined}>
                    <IconButton
                      label={el.locked ? 'Sblocca' : 'Blocca'}
                      noTip
                      onClick={(e) => {
                        e.stopPropagation()
                        board.update(el.id, { locked: !el.locked })
                      }}
                    >
                      {el.locked ? <Lock size={12} /> : <Unlock size={12} />}
                    </IconButton>
                    <IconButton
                      label={el.hidden ? 'Mostra' : 'Nascondi'}
                      noTip
                      onClick={(e) => {
                        e.stopPropagation()
                        board.update(el.id, { hidden: !el.hidden })
                      }}
                    >
                      {el.hidden ? <EyeOff size={12} /> : <Eye size={12} />}
                    </IconButton>
                  </span>
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

/* ---------- templates ---------- */

function TemplateList() {
  return (
    <div className="panel-scroll">
      <div className="tpl-list">
        {TEMPLATES.map((t) => (
          <button key={t.id} type="button" className="tpl-item" onClick={() => commands.insert(t.build(), undefined, { avoidOverlap: true, focus: true })}>
            <TemplateArt template={t} />
            <div>
              <strong>{t.name}</strong>
              <span>{t.description}</span>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}

const artCache = new Map<string, string>()

/** Small picture of a template, drawn once with the board renderer. */
export function TemplateArt({ template }: { template: Template | null }) {
  const key = template?.id ?? 'blank'
  const [url, setUrl] = useState(() => artCache.get(key) ?? '')
  useEffect(() => {
    if (url || !template) return
    let cancelled = false
    document.fonts.ready.then(() => {
      if (cancelled) return
      const els = template.build()
      const b = union(els.map(aabb))
      if (!b) return
      const W = 320
      const H = 200
      const k = Math.min((W - 24) / b.w, (H - 24) / b.h)
      const canvas = document.createElement('canvas')
      canvas.width = W
      canvas.height = H
      const ctx = canvas.getContext('2d')!
      ctx.fillStyle = '#F5F5F5'
      ctx.fillRect(0, 0, W, H)
      ctx.setTransform(k, 0, 0, k, W / 2 - (b.x + b.w / 2) * k, H / 2 - (b.y + b.h / 2) * k)
      for (const el of els) drawElement(ctx, el, { images: null })
      const data = canvas.toDataURL('image/png')
      artCache.set(key, data)
      setUrl(data)
    })
    return () => {
      cancelled = true
    }
  }, [template, url, key])
  return (
    <div className="template-art" aria-hidden="true">
      {template ? (
        url && <img src={url} alt="" width="100%" height="100%" />
      ) : (
        <svg viewBox="0 0 160 100">
          <rect width="160" height="100" fill="var(--bg-2)" />
          <g stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round">
            <line x1="80" y1="40" x2="80" y2="60" />
            <line x1="70" y1="50" x2="90" y2="50" />
          </g>
        </svg>
      )}
    </div>
  )
}
