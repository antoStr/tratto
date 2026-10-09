import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ArrowLeft,
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  Circle,
  Download,
  Eye,
  EyeOff,
  Folder,
  Highlighter,
  Image as ImageIcon,
  ImagePlus,
  Keyboard,
  Lock,
  Minus,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
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
import { toggleFocus, useEditor } from './store.ts'
import { TEMPLATES, type Template } from './templates.ts'
import type { Box, El } from './types.ts'
import { IconButton, Logo, Menu, MenuAt, MenuCheck, MenuContent, MenuItem, MenuSep, MenuSub, MenuTrigger } from '../ui.tsx'
import { SelectionMenu } from './Canvas.tsx'
import { openSettings } from '../Settings.tsx'

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

/** Focus mode: the panels are gone, this small pill stays at the top left to bring them back. */
export function FocusPill(p: Props) {
  const readOnly = useEditor((s) => s.readOnly)
  return (
    <div className="focus-pill">
      <AppMenu host={p.host} readOnly={readOnly} onHome={p.onHome} onNewBoard={p.onNewBoard} onExport={p.onExport} onShortcuts={p.onShortcuts} onInsertImage={p.onInsertImage} />
      <span className="focus-title">{p.title}</span>
      <IconButton label="Mostra i pannelli" kbd="Ctrl+\" onClick={toggleFocus} tipSide="bottom">
        <PanelLeftOpen size={16} />
      </IconButton>
    </div>
  )
}

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
      <IconButton label="Nascondi i pannelli" kbd="Ctrl+\" onClick={toggleFocus} tipSide="bottom">
        <PanelLeftClose size={16} />
      </IconButton>
    </div>
  )
}

function AppMenu({ host, readOnly, onHome, onNewBoard, onExport, onShortcuts, onInsertImage }: { host: boolean; readOnly: boolean; onHome?: () => void; onNewBoard?: () => void; onExport: () => void; onShortcuts: () => void; onInsertImage: () => void }) {
  const prefs = useEditor((s) => s.prefs)
  const setPrefs = useEditor((s) => s.setPrefs)
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
        <MenuItem icon={<Settings2 size={14} />} kbd="Ctrl+," onSelect={openSettings}>
          Impostazioni…
        </MenuItem>
        <MenuItem icon={<PanelsTopLeft size={14} />} kbd="Ctrl+\" onSelect={toggleFocus}>
          {useEditor.getState().prefs.focus ? 'Mostra i pannelli' : 'Nascondi i pannelli'}
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

/** `auto`: strokes written one after the other in the same spot, shown together but not a real folder. */
type Row = { kind: 'el'; el: El; depth: 0 | 1 } | { kind: 'folder'; id: string; name: string; members: El[]; auto?: boolean }

type FolderRow = Extract<Row, { kind: 'folder' }>

const INK = new Set<El['type']>(['ink', 'highlighter'])
/** Strokes farther apart than this (board units) start a new "Scrittura" row. */
const RUN_GAP = 300
const near = (a: Box, b: Box) => a.x - RUN_GAP <= b.x + b.w && b.x - RUN_GAP <= a.x + a.w && a.y - RUN_GAP <= b.y + b.h && b.y - RUN_GAP <= a.y + a.h

/**
 * Layers top to bottom; each folder sits where its top element is, members indented below it when
 * open (`open` null = all open). Consecutive nearby strokes outside folders collapse into one row.
 */
function buildRows(board: Board, open: Set<string> | null): Row[] {
  const isOpen = (id: string) => !open || open.has(id)
  const items = board.all().slice().reverse()
  const byFolder = new Map<string, El[]>()
  for (const el of items) if (el.groupId) byFolder.set(el.groupId, [...(byFolder.get(el.groupId) ?? []), el])
  const rows: Row[] = []
  const container = (id: string, name: string, members: El[], auto?: boolean) => {
    rows.push({ kind: 'folder', id, name, members, auto })
    if (isOpen(id)) for (const m of members) rows.push({ kind: 'el', el: m, depth: 1 })
  }
  let run: El[] = []
  let runBox: Box | null = null
  const flush = () => {
    if (run.length === 1) rows.push({ kind: 'el', el: run[0], depth: 0 })
    // Keyed by the oldest stroke, so the row keeps its open state while more strokes are added.
    else if (run.length > 1) container(`ink:${run[run.length - 1].id}`, run.every((el) => el.type === 'highlighter') ? 'Evidenziature' : 'Scrittura', run, true)
    run = []
    runBox = null
  }
  for (const el of items) {
    if (!el.groupId && INK.has(el.type)) {
      const b = aabb(el)
      if (runBox && !near(runBox, b)) flush()
      run.push(el)
      runBox = runBox ? union([runBox, b])! : b
      continue
    }
    flush()
    const g = el.groupId
    if (!g) rows.push({ kind: 'el', el, depth: 0 })
    else if (byFolder.get(g)![0] === el) container(g, board.groupInfo(g)!.name, byFolder.get(g)!)
  }
  flush()
  return rows
}

const rowIds = (r: Row) => (r.kind === 'el' ? [r.el.id] : r.members.map((m) => m.id))
const rowKey = (r: Row) => (r.kind === 'el' ? r.el.id : `folder:${r.id}`)

function LayerList({ board }: { board: Board }) {
  useBoardVersion(board)
  const selection = useEditor((s) => s.selection)
  const readOnly = useEditor((s) => s.readOnly)
  const scroller = useRef<HTMLDivElement>(null)
  const [scroll, setScroll] = useState({ top: 0, height: 600 })
  const [renaming, setRenaming] = useState<string | null>(null)
  const [open, setOpen] = useState<Set<string>>(() => new Set())
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  const anchor = useRef<string | null>(null)
  const rows = buildRows(board, open)
  const selected = useMemo(() => new Set(selection), [selection])
  const toggleOpen = (id: string, value = !open.has(id)) =>
    setOpen((s) => {
      const next = new Set(s)
      if (value) next.add(id)
      else next.delete(id)
      return next
    })

  useEffect(() => {
    const el = scroller.current!
    const ro = new ResizeObserver(() => setScroll((s) => ({ ...s, height: el.clientHeight })))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // A new folder opens with its name ready to type.
  useEffect(() => {
    const onCreated = (e: Event) => {
      const id = (e as CustomEvent<string>).detail
      toggleOpen(id, true)
      setRenaming(`folder:${id}`)
    }
    window.addEventListener('tratto:folder-created', onCreated)
    return () => window.removeEventListener('tratto:folder-created', onCreated)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Selecting on the canvas opens the folder and brings the row into view.
  useEffect(() => {
    const last = selection.length ? board.get(selection[selection.length - 1]) : null
    if (!last) return
    // The folder (or "Scrittura" row) holding it; a whole folder selected from its row stays closed.
    const all = buildRows(board, null)
    const parent = all.find((r): r is FolderRow => r.kind === 'folder' && r.members.some((m) => m.id === last.id))
    const part = !!parent && !parent.members.every((m) => selected.has(m.id))
    if (parent && part && !open.has(parent.id)) toggleOpen(parent.id, true)
    const target = parent && !part ? rowKey(parent) : last.id
    const el = scroller.current
    const i = buildRows(board, parent && part ? new Set([...open, parent.id]) : open).findIndex((r) => rowKey(r) === target)
    if (!el || i < 0) return
    const top = i * ROW
    if (top < el.scrollTop || top + ROW > el.scrollTop + el.clientHeight) el.scrollTop = top - el.clientHeight / 2
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection])

  const first = Math.max(0, Math.floor(scroll.top / ROW) - 5)
  const last = Math.min(rows.length, Math.ceil((scroll.top + scroll.height) / ROW) + 5)

  const click = (e: React.MouseEvent, row: Row) => {
    const set = useEditor.setState
    const ids = rowIds(row)
    if (e.shiftKey && anchor.current) {
      const a = rows.findIndex((r) => rowKey(r) === anchor.current)
      const b = rows.indexOf(row)
      const [from, to] = a < b ? [a, b] : [b, a]
      set({ selection: [...new Set(rows.slice(from, to + 1).flatMap(rowIds))], tool: 'select' })
      return
    }
    anchor.current = rowKey(row)
    if (e.ctrlKey || e.metaKey) {
      const all = ids.every((id) => selected.has(id))
      set({ selection: all ? selection.filter((id) => !ids.includes(id)) : [...new Set([...selection, ...ids])], tool: 'select' })
      return
    }
    set({ selection: ids, tool: 'select' })
    commands.reveal(ids)
  }

  const contextMenu = (e: React.MouseEvent, row: Row) => {
    e.preventDefault()
    if (readOnly) return
    const ids = rowIds(row)
    if (!ids.every((id) => selected.has(id))) useEditor.setState({ selection: ids, tool: 'select' })
    setMenu({ x: e.clientX, y: e.clientY })
  }

  const finishRename = (row: Row, value: string) => {
    setRenaming(null)
    if (row.kind === 'folder') {
      if (!row.auto) board.renameGroup(row.id, value)
    }
    else board.update(row.el.id, { name: value.trim().slice(0, 80) || undefined })
  }

  return (
    <div className="panel-scroll" ref={scroller} onScroll={(e) => setScroll({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}>
      {!rows.length ? (
        <p className="layers-empty">Ancora niente qui. Tutto quello che disegni o aggiungi comparirà in questo elenco.</p>
      ) : (
        <div className="layers" role="list" aria-label="Livelli" style={{ height: rows.length * ROW + 12 }}>
          {rows.slice(first, last).map((row, k) => {
            const i = first + k
            const key = rowKey(row)
            const ids = rowIds(row)
            const els = row.kind === 'el' ? [row.el] : row.members
            const locked = els.every((el) => el.locked)
            const hidden = els.every((el) => el.hidden)
            const isSel = ids.every((id) => selected.has(id))
            const label = row.kind === 'el' ? elementLabel(row.el) : row.name
            return (
              <div
                key={key}
                role="listitem"
                data-selected={isSel}
                className="layer-row"
                data-folder={row.kind === 'folder' || undefined}
                data-depth={row.kind === 'el' ? row.depth : 0}
                data-dim={hidden || undefined}
                style={{ top: 4 + i * ROW }}
                onClick={(e) => click(e, row)}
                onDoubleClick={() => !readOnly && !(row.kind === 'folder' && row.auto) && setRenaming(key)}
                onContextMenu={(e) => contextMenu(e, row)}
              >
                {row.kind === 'folder' ? (
                  <button
                    type="button"
                    className="layer-chevron"
                    aria-expanded={open.has(row.id)}
                    aria-label={`${open.has(row.id) ? 'Chiudi' : 'Apri'} ${row.name}`}
                    onClick={(e) => {
                      e.stopPropagation()
                      toggleOpen(row.id)
                    }}
                    onDoubleClick={(e) => e.stopPropagation()}
                  >
                    <ChevronRight size={12} />
                  </button>
                ) : null}
                <span className="layer-icon" aria-hidden="true">
                  {row.kind === 'folder' ? row.auto ? <PenLine size={14} /> : <Folder size={14} /> : typeIcon(row.el)}
                </span>
                {renaming === key ? (
                  <input
                    className="rename-input"
                    aria-label={row.kind === 'folder' ? 'Nome della cartella' : 'Nome del livello'}
                    defaultValue={label}
                    autoFocus
                    onFocus={(e) => e.target.select()}
                    onClick={(e) => e.stopPropagation()}
                    onBlur={(e) => finishRename(row, e.target.value)}
                    onKeyDown={(e) => {
                      e.stopPropagation()
                      if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                      if (e.key === 'Escape') setRenaming(null)
                    }}
                  />
                ) : (
                  // A real button, so the list works with Tab and Enter (F2 renames).
                  <button
                    type="button"
                    className="layer-name"
                    aria-pressed={isSel}
                    onKeyDown={(e) => {
                      if (e.key === 'F2' && !readOnly && !(row.kind === 'folder' && row.auto)) setRenaming(key)
                    }}
                  >
                    {label}
                    {row.kind === 'folder' && <span className="layer-count">{row.members.length}</span>}
                  </button>
                )}
                {!readOnly && (
                  <span className="layer-actions" data-pinned={locked || hidden || undefined}>
                    <IconButton
                      label={locked ? 'Sblocca' : 'Blocca'}
                      noTip
                      onClick={(e) => {
                        e.stopPropagation()
                        board.undo.stopCapturing()
                        board.updateMany(new Map(els.map((el) => [el.id, { locked: !locked }])))
                      }}
                    >
                      {locked ? <Lock size={12} /> : <Unlock size={12} />}
                    </IconButton>
                    <IconButton
                      label={hidden ? 'Mostra' : 'Nascondi'}
                      noTip
                      onClick={(e) => {
                        e.stopPropagation()
                        board.undo.stopCapturing()
                        board.updateMany(new Map(els.map((el) => [el.id, { hidden: !hidden }])))
                      }}
                    >
                      {hidden ? <EyeOff size={12} /> : <Eye size={12} />}
                    </IconButton>
                  </span>
                )}
              </div>
            )
          })}
        </div>
      )}
      <MenuAt at={menu} onClose={() => setMenu(null)}>
        <SelectionMenu board={board} />
      </MenuAt>
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
