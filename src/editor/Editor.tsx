import { useCallback, useEffect, useRef, useState } from 'react'
import * as Y from 'yjs'
import { HocuspocusProvider } from '@hocuspocus/provider'
import { api, fetchFile, uploadImage } from '../api.ts'
import { Dialog, toast } from '../ui.tsx'
import { Board, uid } from './doc.ts'
import { Canvas } from './Canvas.tsx'
import { commands } from './controller.ts'
import { ExportDialog } from './ExportDialog.tsx'
import { thumbnail } from './export.ts'
import type { Pt } from './geometry.ts'
import { RightPanel } from './Inspector.tsx'
import { FocusPill, LeftPanel, type Status } from './Layers.tsx'
import { Minimap } from './Minimap.tsx'
import { ImageStore } from './render.ts'
import { JoinRequests, ShareDialog, useShare, useSharePolling } from './ShareDialog.tsx'
import { useEditor } from './store.ts'
import { TEMPLATES } from './templates.ts'
import { Toolbar } from './Toolbar.tsx'
import type { ImageEl } from './types.ts'
import './editor.css'

export interface GuestSession {
  code: string
  ticket: string
  name: string
  access: 'edit' | 'view'
}

interface Props {
  boardId: string
  title: string
  guest?: GuestSession
  template?: string | null
  onTitle?: (title: string) => void
  onHome?: () => void
  onNewBoard?: () => void
  /** Guest only: the host closed the session or removed us. */
  onEnded?: (reason: 'ended' | 'removed') => void
}

const GUEST_COLORS = ['#9747FF', '#F24822', '#14AE5C', '#FFA629', '#E84393', '#00B5CE', '#7B61FF', '#FF7262']
const colorFor = (seed: string) => GUEST_COLORS[[...seed].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7) % GUEST_COLORS.length]

interface Session {
  doc: Y.Doc
  board: Board
  provider: HocuspocusProvider
  images: ImageStore
}

export function Editor({ boardId, title, guest, template, onTitle, onHome, onNewBoard, onEnded }: Props) {
  const host = !guest
  const [session, setSession] = useState<Session | null>(null)
  const [status, setStatus] = useState<Status>('connecting')
  const [shareOpen, setShareOpen] = useState(false)
  const [exportOpen, setExportOpen] = useState<false | 'all' | 'selection'>(false)
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  const prefs = useEditor((s) => s.prefs)
  const ui = !prefs.focus
  const following = useEditor((s) => s.following)
  const name = useEditor((s) => s.prefs.name)
  const sharing = useShare((s) => s.state.active && s.state.boardId === boardId)
  const repaint = useRef<() => void>(() => {})
  const hadCamera = useRef(true)
  const fileInput = useRef<HTMLInputElement>(null)
  const endedRef = useRef(onEnded)
  endedRef.current = onEnded

  if (host) useSharePolling() // eslint-disable-line react-hooks/rules-of-hooks -- `host` never changes for a mounted editor

  const getFile = useCallback((fileId: string) => fetchFile(boardId, fileId), [boardId])

  /* ---------- connection ---------- */
  useEffect(() => {
    const doc = new Y.Doc()
    const board = new Board(doc)
    const images = new ImageStore(getFile, () => repaint.current())
    const proto = location.protocol === 'https:' ? 'wss' : 'ws'
    let synced = false
    const provider = new HocuspocusProvider({
      url: `${proto}://${location.host}/collab`,
      name: boardId,
      document: doc,
      token: guest ? `${guest.code}.${guest.ticket}` : 'host',
      flushDelay: 40,
      onStatus: ({ status }) => setStatus(status === 'connected' ? (synced ? 'synced' : 'connecting') : status === 'connecting' ? 'connecting' : 'offline'),
      onSynced: () => {
        synced = true
        setStatus('synced')
      },
      onAuthenticated: ({ scope }) => useEditor.setState({ readOnly: scope === 'readonly' }),
      onAuthenticationFailed: () => endedRef.current?.('removed'),
      onClose: ({ event }) => {
        if (guest && event.code === 4001) endedRef.current?.('ended')
        if (guest && event.code === 4003) endedRef.current?.('removed')
      },
    })
    useEditor.setState({ selection: [], editingId: null, following: null, readOnly: guest?.access === 'view', tool: guest?.access === 'view' ? 'select' : 'pen' })
    setSession({ doc, board, provider, images })
    return () => {
      provider.destroy()
      doc.destroy()
      setSession(null)
    }
  }, [boardId, guest, getFile])

  /* ---------- who I am, for the others ---------- */
  useEffect(() => {
    session?.provider.awareness?.setLocalStateField('user', {
      name: (guest ? guest.name : name.trim() || 'Proprietario').slice(0, 40),
      color: guest ? colorFor(guest.ticket) : '#0D99FF',
    })
  }, [session, guest, name])

  /* ---------- first sync: template, camera ---------- */
  useEffect(() => {
    if (!session || status !== 'synced') return
    const { board } = session
    // A brand-new board gets the background chosen in Settings (not an undoable edit).
    if (host && !board.all().length && !board.meta.size)
      board.doc.transact(() => {
        for (const [k, v] of Object.entries(useEditor.getState().prefs.newBoard)) board.meta.set(k, v)
      })
    const tpl = template && TEMPLATES.find((t) => t.id === template)
    if (tpl && !board.all().length) {
      commands.insert(tpl.build(), { x: 0, y: 0 })
      useEditor.setState({ selection: [], tool: 'select' })
      hadCamera.current = false
    }
    if (!hadCamera.current) {
      hadCamera.current = true
      requestAnimationFrame(() => (board.all().length ? commands.fit() : commands.zoomTo(1)))
    }
    if (template) history.replaceState(null, '', location.pathname)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session, status === 'synced'])

  /* ---------- thumbnails for the board list (host) ---------- */
  useEffect(() => {
    if (!session || !host) return
    let timer = 0
    let dirty = false
    const save = async () => {
      dirty = false
      try {
        const blob = await thumbnail(session.board, getFile)
        if (blob) await api(`/api/boards/${boardId}/thumbnail`, { method: 'PUT', body: blob })
      } catch {
        /* the list just shows the previous picture */
      }
    }
    const unsub = session.board.subscribe(() => {
      dirty = true
      clearTimeout(timer)
      timer = window.setTimeout(save, 3000)
    })
    return () => {
      unsub()
      clearTimeout(timer)
      if (dirty) save()
    }
  }, [session, host, boardId, getFile])

  /* ---------- images ---------- */
  const addImages = useCallback(
    async (files: File[], at: Pt) => {
      if (!session || useEditor.getState().readOnly) return
      const z = useEditor.getState().camera.z
      const els: ImageEl[] = []
      for (const file of files.slice(0, 20)) {
        try {
          const up = await uploadImage(boardId, file)
          session.images.prime(up.id, up.bitmap)
          const k = Math.min(1, 640 / Math.max(up.width, up.height)) / z
          const w = up.width * k
          const h = up.height * k
          const offset = (els.length * 24) / z
          els.push({ id: uid(), type: 'image', fileId: up.id, x: at.x - w / 2 + offset, y: at.y - h / 2 + offset, w, h, rotation: 0, z: 0, opacity: 1 })
        } catch (e) {
          toast(e instanceof Error ? e.message : 'Non sono riuscito a caricare l’immagine.', 'error')
        }
      }
      if (els.length) commands.insert(els, at)
    },
    [session, boardId],
  )

  /* ---------- global shortcuts and events ---------- */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement
      if (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName)) return
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'e') {
        e.preventDefault()
        setExportOpen('all')
      } else if (e.key === '?' || ((e.ctrlKey || e.metaKey) && e.key === '/')) {
        e.preventDefault()
        setShortcutsOpen(true)
      }
    }
    const onExport = (e: Event) => setExportOpen((e as CustomEvent).detail === 'selection' ? 'selection' : 'all')
    const onImage = () => fileInput.current?.click()
    window.addEventListener('keydown', onKey)
    window.addEventListener('tratto:export', onExport)
    window.addEventListener('tratto:insert-image', onImage)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('tratto:export', onExport)
      window.removeEventListener('tratto:insert-image', onImage)
    }
  }, [])

  useEffect(() => {
    document.title = `${title} – Tratto`
  }, [title])

  // A guest whose host vanished (app closed, PC off) gets a clear end screen instead of endless retries.
  useEffect(() => {
    if (!guest || status !== 'offline') return
    const t = setTimeout(() => endedRef.current?.('ended'), 20_000)
    return () => clearTimeout(t)
  }, [guest, status])

  if (!session) return null
  const { board, provider, images } = session

  const rename = async (t: string) => {
    try {
      await api(`/api/boards/${boardId}`, { method: 'PATCH', json: { title: t } })
      onTitle?.(t)
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  return (
    <>
      <div className="editor" data-ui={ui} data-left={prefs.leftPanel} data-right={prefs.rightPanel}>
        <LeftPanel
          board={board}
          title={title}
          host={host}
          status={status}
          onRename={host ? rename : undefined}
          onHome={onHome}
          onNewBoard={onNewBoard}
          onExport={() => setExportOpen('all')}
          onShortcuts={() => setShortcutsOpen(true)}
          onInsertImage={() => fileInput.current?.click()}
        />
        <main className="stage" data-toolbar={prefs.toolbarPos}>
          <Canvas
            board={board}
            awareness={provider.awareness}
            images={images}
            boardKey={boardId}
            onImageFiles={addImages}
            onReady={({ hadSavedCamera, invalidate }) => {
              hadCamera.current = hadSavedCamera
              repaint.current = invalidate
            }}
          />
          <Toolbar onImage={() => fileInput.current?.click()} />
          {ui && prefs.minimap && <Minimap board={board} images={images} />}
          {!ui && (
            <FocusPill
              board={board}
              title={title}
              host={host}
              status={status}
              onHome={onHome}
              onNewBoard={onNewBoard}
              onExport={() => setExportOpen('all')}
              onShortcuts={() => setShortcutsOpen(true)}
              onInsertImage={() => fileInput.current?.click()}
            />
          )}
          {host && !shareOpen && <JoinRequests />}
        </main>
        <RightPanel board={board} awareness={provider.awareness} host={host} access={guest?.access ?? 'edit'} following={following} sharing={sharing} onShare={() => setShareOpen(true)} onExport={() => setExportOpen('all')} />
      </div>

      <input
        ref={fileInput}
        type="file"
        accept="image/png,image/jpeg,image/gif,image/webp"
        multiple
        hidden
        onChange={(e) => {
          const files = [...(e.target.files ?? [])]
          e.target.value = ''
          if (files.length) addImages(files, commands.viewportCenter())
        }}
      />
      {host && <ShareDialog open={shareOpen} onOpenChange={setShareOpen} boardId={boardId} title={title} />}
      <ExportDialog open={!!exportOpen} onOpenChange={(o) => !o && setExportOpen(false)} board={board} boardId={boardId} title={title} host={host} fetchFile={getFile} selectionFirst={exportOpen === 'selection'} />
      <ShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
    </>
  )
}

/* ---------- keyboard shortcuts ---------- */

const SHORTCUTS: [string, [string, string][]][] = [
  [
    'Strumenti',
    [
      ['Seleziona', 'V'],
      ['Mano', 'H  ·  Spazio'],
      ['Penna', 'P'],
      ['Penne 1–4', '1 2 3 4'],
      ['Evidenziatore', 'M'],
      ['Gomma', 'E'],
      ['Lazo', 'Q'],
      ['Righello', 'U'],
      ['Rettangolo / Ellisse', 'R / O'],
      ['Linea / Freccia', 'L / Maiusc+L'],
      ['Testo', 'T'],
      ['Nota adesiva', 'S'],
      ['Immagine', 'I'],
      ['Puntatore laser', 'K'],
    ],
  ],
  [
    'Modifica',
    [
      ['Annulla', 'Ctrl+Z'],
      ['Ripeti', 'Ctrl+Maiusc+Z'],
      ['Copia / Incolla', 'Ctrl+C / Ctrl+V'],
      ['Duplica', 'Ctrl+D'],
      ['Elimina', 'Canc'],
      ['Seleziona tutto', 'Ctrl+A'],
      ['Metti in una cartella', 'Ctrl+G'],
      ['Togli dalla cartella', 'Ctrl+Maiusc+G'],
      ['Sposta di 1 / 10', 'Frecce / Maiusc+Frecce'],
      ['Porta avanti / indietro', 'Ctrl+] / Ctrl+['],
      ['Blocca', 'Ctrl+Maiusc+L'],
      ['Nascondi', 'Ctrl+Maiusc+H'],
      ['Duplica trascinando', 'Alt + trascina'],
      ['Senza aggancio', 'Ctrl + trascina'],
    ],
  ],
  [
    'Vista',
    [
      ['Zoom avanti / indietro', 'Ctrl++ / Ctrl+−'],
      ['Zoom con rotellina', 'Ctrl + rotellina'],
      ['Adatta alla lavagna', 'Maiusc+1'],
      ['Adatta alla selezione', 'Maiusc+2'],
      ['Zoom 100%', 'Maiusc+0'],
      ['Nascondi o mostra i pannelli', 'Ctrl+\\'],
      ['Esporta', 'Ctrl+Maiusc+E'],
      ['Impostazioni', 'Ctrl+,'],
    ],
  ],
]

function ShortcutsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Scorciatoie da tastiera" wide>
      <div className="shortcuts">
        {SHORTCUTS.map(([group, list]) => (
          <div key={group} style={{ display: 'contents' }}>
            <h4>{group}</h4>
            {list.map(([label, keys]) => (
              <div key={label} className="shortcut">
                <span>{label}</span>
                <kbd>{keys}</kbd>
              </div>
            ))}
          </div>
        ))}
      </div>
      <p className="hint" style={{ marginTop: 16 }}>
        Con la penna: il tasto laterale seleziona col lazo (un tocco apre il menu), la parte superiore cancella. Tieni premuto per aprire il menu. Con due dita sposti e ingrandisci la lavagna.
      </p>
    </Dialog>
  )
}
