import { useEffect, useState } from 'react'
import { Copy, Download } from 'lucide-react'
import { api } from '../api.ts'
import { Dialog, Segmented, Switch, toast } from '../ui.tsx'
import type { Board } from './doc.ts'
import { download, exportPDF, exportPNG, exportSVG, safeFilename, thumbnail } from './export.ts'
import { useEditor } from './store.ts'

type Format = 'png' | 'jpg' | 'svg' | 'pdf' | 'tratto'

interface Props {
  open: boolean
  onOpenChange: (o: boolean) => void
  board: Board
  boardId: string
  title: string
  host: boolean
  fetchFile: (fileId: string) => Promise<Blob>
  /** Start with "only the selection" ticked. */
  selectionFirst: boolean
}

export function ExportDialog({ open, onOpenChange, board, boardId, title, host, fetchFile, selectionFirst }: Props) {
  const selection = useEditor((s) => s.selection)
  const [format, setFormat] = useState<Format>('png')
  const [scale, setScale] = useState('2')
  const [background, setBackground] = useState(true)
  const [onlySelection, setOnlySelection] = useState(false)
  const [page, setPage] = useState<'fit' | 'a4'>('fit')
  const [busy, setBusy] = useState(false)
  const [preview, setPreview] = useState<string | null>(null)

  useEffect(() => {
    if (open) setOnlySelection(selectionFirst && selection.length > 0)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  useEffect(() => {
    if (!open) return
    let url: string | null = null
    let cancelled = false
    thumbnail(board, fetchFile, onlySelection ? selection : null)
      .then((b) => {
        if (b && !cancelled) setPreview((url = URL.createObjectURL(b)))
      })
      .catch(() => {})
    return () => {
      cancelled = true
      if (url) URL.revokeObjectURL(url)
      setPreview(null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, board, fetchFile, onlySelection])

  const name = safeFilename(title)
  const run = async () => {
    setBusy(true)
    try {
      const o = { ids: onlySelection ? selection : null, scale: Number(scale), background, fetchFile, page }
      if (format === 'png') download(await exportPNG(board, o), `${name}.png`)
      else if (format === 'jpg') download(await exportPNG(board, { ...o, type: 'image/jpeg' }), `${name}.jpg`)
      else if (format === 'svg') download(await exportSVG(board, o), `${name}.svg`, 'image/svg+xml')
      else if (format === 'pdf') download(await exportPDF(board, o), `${name}.pdf`)
      else {
        const data = await api<unknown>(`/api/boards/${boardId}/export`)
        download(JSON.stringify(data), `${name}.tratto`, 'application/json')
      }
      toast('Esportazione pronta.')
      onOpenChange(false)
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  // Image on the clipboard, to paste straight into chats, documents and slides.
  const copy = async () => {
    setBusy(true)
    try {
      const png = await exportPNG(board, { ids: onlySelection ? selection : null, scale: Number(scale), background, fetchFile })
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })])
      toast('Immagine copiata: incollala dove vuoi con Ctrl+V.')
      onOpenChange(false)
    } catch (e) {
      toast(e instanceof Error && e.message.includes('vuota') ? e.message : 'Non riesco a copiare l’immagine: usa Esporta.', 'error')
    } finally {
      setBusy(false)
    }
  }

  const formats: { value: Format; label: string; title: string }[] = [
    { value: 'png', label: 'PNG', title: 'Immagine di alta qualità, anche con sfondo trasparente' },
    { value: 'jpg', label: 'JPG', title: 'Immagine leggera, la più facile da mandare in chat o per e-mail' },
    { value: 'svg', label: 'SVG', title: 'Vettoriale, modificabile in altri programmi' },
    { value: 'pdf', label: 'PDF', title: 'Documento da stampare o inviare' },
  ]
  if (host) formats.push({ value: 'tratto', label: 'Tratto', title: 'Copia completa da riaprire in Tratto' })

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Esporta"
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={() => onOpenChange(false)}>
            Annulla
          </button>
          {format !== 'tratto' && 'ClipboardItem' in window && (
            <button type="button" className="btn btn-secondary" onClick={copy} disabled={busy}>
              <Copy size={12} /> Copia
            </button>
          )}
          <button type="button" className="btn btn-primary" onClick={run} disabled={busy}>
            {busy ? <span className="spinner" aria-hidden="true" /> : <Download size={12} />}
            Esporta
          </button>
        </>
      }
    >
      <div className="export-preview">{preview ? <img src={preview} alt="Anteprima della lavagna" /> : <span className="hint">Nessuna anteprima: la lavagna è vuota.</span>}</div>
      <div className="form-row">
        <span className="label">Formato</span>
        <Segmented label="Formato" value={format} onChange={setFormat} options={formats} />
        <p className="hint">{formats.find((f) => f.value === format)?.title}.</p>
      </div>
      {format !== 'tratto' && (
        <>
          {format === 'pdf' && (
            <div className="form-row">
              <span className="label">Pagina</span>
              <Segmented
                label="Pagina"
                value={page}
                onChange={setPage}
                options={[
                  { value: 'fit', label: 'Come il contenuto' },
                  { value: 'a4', label: 'A4 da stampare' },
                ]}
              />
            </div>
          )}
          {format !== 'svg' && (
            <div className="form-row">
              <span className="label">Risoluzione</span>
              <Segmented
                label="Risoluzione"
                value={scale}
                onChange={setScale}
                options={[
                  { value: '1', label: '1×' },
                  { value: '2', label: '2×' },
                  { value: '3', label: '3×' },
                  { value: '4', label: '4×' },
                ]}
              />
            </div>
          )}
          {format !== 'jpg' && <Switch id="exp-bg" label="Includi lo sfondo" checked={background} onChange={setBackground} />}
          <Switch id="exp-sel" label={`Solo la selezione${selection.length ? ` (${selection.length})` : ''}`} checked={onlySelection && selection.length > 0} onChange={(v) => setOnlySelection(v && selection.length > 0)} />
        </>
      )}
      {format === 'tratto' && <p className="hint">Il file contiene tutta la lavagna con le immagini. Puoi riaprirlo da «Importa» nella schermata delle lavagne, anche su un altro PC.</p>}
    </Dialog>
  )
}
