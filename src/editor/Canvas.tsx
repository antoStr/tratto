import { useEffect, useRef, useState } from 'react'
import { DropdownMenu } from 'radix-ui'
import { ArrowDownToLine, ArrowUpToLine, ClipboardPaste, Copy, CopyPlus, EyeOff, Lock, Maximize, MousePointerSquareDashed, Ruler, Scissors, Trash2, Unlock } from 'lucide-react'
import type { Board } from './doc.ts'
import { commands, createController, type Awareness } from './controller.ts'
import { Cursors, RulerView, TextEditor } from './overlays.tsx'
import type { ImageStore } from './render.ts'
import { useEditor } from './store.ts'
import type { Pt } from './geometry.ts'
import { MenuContent, MenuItem, MenuSep } from '../ui.tsx'

interface Props {
  board: Board
  awareness: Awareness | null
  images: ImageStore
  boardKey: string
  onImageFiles: (files: File[], at: Pt) => void
  onReady?: (info: { hadSavedCamera: boolean; invalidate: () => void }) => void
}

export function Canvas({ board, awareness, images, boardKey, onImageFiles, onReady }: Props) {
  const root = useRef<HTMLDivElement>(null)
  const scene = useRef<HTMLCanvasElement>(null)
  const overlay = useRef<HTMLCanvasElement>(null)
  const filesRef = useRef(onImageFiles)
  filesRef.current = onImageFiles
  const readyRef = useRef(onReady)
  readyRef.current = onReady
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)

  useEffect(() => {
    const c = createController({
      root: root.current!,
      scene: scene.current!,
      overlay: overlay.current!,
      board,
      awareness,
      images,
      boardKey,
      onImageFiles: (f, at) => filesRef.current(f, at),
    })
    readyRef.current?.({ hadSavedCamera: c.hadSavedCamera, invalidate: () => c.invalidate(true) })
    return () => c.destroy()
  }, [board, awareness, images, boardKey])

  return (
    <div
      ref={root}
      className="canvas-root"
      onContextMenu={(e) => {
        e.preventDefault()
        // Only a real right click opens the menu: a pen held still must keep drawing.
        if ((e.nativeEvent as PointerEvent).pointerType && (e.nativeEvent as PointerEvent).pointerType !== 'mouse') return
        const r = root.current!.getBoundingClientRect()
        setMenu({ x: e.clientX - r.left, y: e.clientY - r.top })
      }}
    >
      <canvas ref={scene} className="canvas-layer" aria-hidden="true" />
      <canvas ref={overlay} className="canvas-layer" role="img" aria-label="Lavagna. Usa gli strumenti della barra in basso per disegnare." />
      <RulerView />
      <Cursors awareness={awareness} />
      <TextEditor board={board} />
      <DropdownMenu.Root open={!!menu} onOpenChange={(o) => !o && setMenu(null)} modal={false}>
        <DropdownMenu.Trigger asChild>
          <span className="menu-anchor" style={{ left: menu?.x ?? 0, top: menu?.y ?? 0 }} />
        </DropdownMenu.Trigger>
        {menu && <CanvasMenu board={board} />}
      </DropdownMenu.Root>
    </div>
  )
}

function CanvasMenu({ board }: { board: Board }) {
  const selection = useEditor((s) => s.selection)
  const readOnly = useEditor((s) => s.readOnly)
  const ruler = useEditor((s) => s.ruler.visible)
  const has = selection.length > 0
  if (readOnly)
    return (
      <MenuContent>
        <MenuItem icon={<Maximize size={14} />} kbd="Maiusc+1" onSelect={() => commands.fit()}>
          Adatta alla lavagna
        </MenuItem>
      </MenuContent>
    )
  return (
    <MenuContent>
      {has ? (
        <>
          <MenuItem icon={<Copy size={14} />} kbd="Ctrl+C" onSelect={() => commands.copy()}>
            Copia
          </MenuItem>
          <MenuItem icon={<Scissors size={14} />} kbd="Ctrl+X" onSelect={() => commands.cut()}>
            Taglia
          </MenuItem>
          <MenuItem icon={<ClipboardPaste size={14} />} kbd="Ctrl+V" onSelect={() => commands.paste()}>
            Incolla
          </MenuItem>
          <MenuItem icon={<CopyPlus size={14} />} kbd="Ctrl+D" onSelect={() => commands.duplicate()}>
            Duplica
          </MenuItem>
          <MenuSep />
          <MenuItem icon={<ArrowUpToLine size={14} />} kbd="Ctrl+Maiusc+]" onSelect={() => commands.order('front')}>
            Porta in primo piano
          </MenuItem>
          <MenuItem kbd="Ctrl+]" onSelect={() => commands.order('forward')}>
            Porta avanti
          </MenuItem>
          <MenuItem kbd="Ctrl+[" onSelect={() => commands.order('backward')}>
            Porta indietro
          </MenuItem>
          <MenuItem icon={<ArrowDownToLine size={14} />} kbd="Ctrl+Maiusc+[" onSelect={() => commands.order('back')}>
            Porta in fondo
          </MenuItem>
          <MenuSep />
          <MenuItem icon={<Lock size={14} />} kbd="Ctrl+Maiusc+L" onSelect={() => commands.toggleLock()}>
            Blocca
          </MenuItem>
          <MenuItem icon={<EyeOff size={14} />} kbd="Ctrl+Maiusc+H" onSelect={() => commands.toggleHide()}>
            Nascondi
          </MenuItem>
          <MenuItem icon={<Maximize size={14} />} kbd="Maiusc+2" onSelect={() => commands.fitSelection()}>
            Zoom sulla selezione
          </MenuItem>
          <MenuItem onSelect={() => window.dispatchEvent(new CustomEvent('tratto:export', { detail: 'selection' }))}>Esporta selezione…</MenuItem>
          <MenuSep />
          <MenuItem icon={<Trash2 size={14} />} kbd="Canc" danger onSelect={() => commands.remove()}>
            Elimina
          </MenuItem>
        </>
      ) : (
        <>
          <MenuItem icon={<ClipboardPaste size={14} />} kbd="Ctrl+V" onSelect={() => commands.paste()}>
            Incolla
          </MenuItem>
          <MenuItem icon={<MousePointerSquareDashed size={14} />} kbd="Ctrl+A" onSelect={() => commands.selectAll()}>
            Seleziona tutto
          </MenuItem>
          <MenuSep />
          <MenuItem icon={<Maximize size={14} />} kbd="Maiusc+1" onSelect={() => commands.fit()}>
            Adatta alla lavagna
          </MenuItem>
          <MenuItem icon={<Ruler size={14} />} kbd="U" onSelect={() => useEditor.getState().setRuler({ visible: !ruler })}>
            {ruler ? 'Nascondi righello' : 'Mostra righello'}
          </MenuItem>
          <MenuItem icon={<Unlock size={14} />} onSelect={() => unlockAll(board)}>
            Sblocca e mostra tutto
          </MenuItem>
        </>
      )}
    </MenuContent>
  )
}

function unlockAll(board: Board) {
  const hiddenOrLocked = board.all().filter((el) => el.locked || el.hidden)
  board.undo.stopCapturing()
  board.updateMany(new Map(hiddenOrLocked.map((el) => [el.id, { locked: false, hidden: false }])))
}
