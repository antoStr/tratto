import { useEffect, useMemo, useRef, useState } from 'react'
import { Clock, Copy, Download, FileUp, LayoutTemplate, MoreHorizontal, PenLine, Pencil, Plus, Search, Settings2, Trash2 } from 'lucide-react'
import { api, timeAgo, type BoardInfo } from '../api.ts'
import { download, safeFilename } from '../editor/export.ts'
import { TemplateArt } from '../editor/Layers.tsx'
import { useShare, useSharePolling } from '../editor/ShareDialog.tsx'
import { TEMPLATES } from '../editor/templates.ts'
import { openSettings } from '../Settings.tsx'
import { Dialog, IconButton, Logo, Menu, MenuContent, MenuItem, MenuSep, MenuTrigger, toast, useTick } from '../ui.tsx'

type Sort = 'updated' | 'created' | 'title'
const SORT_LABEL: Record<Sort, string> = { updated: 'Ultima modifica', created: 'Data di creazione', title: 'Nome' }

export function Home({ onOpen }: { onOpen: (id: string, template?: string) => void }) {
  const [boards, setBoards] = useState<BoardInfo[] | null>(null)
  const [query, setQuery] = useState('')
  const [sort, setSort] = useState<Sort>('updated')
  const [renaming, setRenaming] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<BoardInfo | null>(null)
  const importInput = useRef<HTMLInputElement>(null)
  const templatesRef = useRef<HTMLElement>(null)
  const sharedBoard = useShare((s) => (s.state.active ? s.state.boardId : null))
  useSharePolling()
  useTick(60_000)

  const load = () =>
    api<BoardInfo[]>('/api/boards')
      .then(setBoards)
      .catch((e) => {
        setBoards([])
        toast(e.message, 'error')
      })
  useEffect(() => {
    load()
    document.title = 'Lavagne – Tratto'
  }, [])

  const create = async (template?: string) => {
    const t = template ? TEMPLATES.find((x) => x.id === template)?.name : undefined
    try {
      const { id } = await api<{ id: string }>('/api/boards', { method: 'POST', json: { title: t ?? 'Lavagna senza titolo' } })
      onOpen(id, template)
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    const list = (boards ?? []).filter((b) => !q || b.title.toLowerCase().includes(q))
    return list.sort((a, b) => (sort === 'title' ? a.title.localeCompare(b.title, 'it') : sort === 'created' ? b.created_at - a.created_at : b.updated_at - a.updated_at))
  }, [boards, query, sort])

  const act = async (fn: () => Promise<unknown>, done?: string) => {
    try {
      await fn()
      if (done) toast(done)
      await load()
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  const importFile = async (file: File) => {
    try {
      const data = JSON.parse(await file.text())
      const { id } = await api<{ id: string }>('/api/boards/import', { method: 'POST', json: data })
      toast('Lavagna importata.')
      onOpen(id)
    } catch (e) {
      toast(e instanceof SyntaxError ? 'Questo file non è una lavagna di Tratto.' : e instanceof Error ? e.message : String(e), 'error')
    }
  }

  return (
    <div className="home">
      <nav className="home-side" aria-label="Sezioni">
        <div className="home-brand">
          <Logo size={24} />
          Tratto
        </div>
        <button type="button" className="side-item" aria-current="page">
          <Clock size={16} /> Recenti
        </button>
        <button type="button" className="side-item" onClick={() => templatesRef.current?.scrollIntoView({ behavior: 'smooth' })}>
          <LayoutTemplate size={16} /> Modelli
        </button>
        <div className="side-foot">
          <button type="button" className="side-item" style={{ width: '100%' }} onClick={() => importInput.current?.click()}>
            <FileUp size={16} /> Importa una lavagna
          </button>
          <button type="button" className="side-item" style={{ width: '100%' }} onClick={openSettings}>
            <Settings2 size={16} /> Impostazioni
          </button>
        </div>
      </nav>

      <main className="home-main">
        <header className="home-head">
          <h1 className="home-title">Recenti</h1>
          <label className="field home-search">
            <Search size={14} aria-hidden="true" />
            <span className="sr-only">Cerca lavagne</span>
            <input type="search" placeholder="Cerca" value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <Menu>
            <MenuTrigger asChild>
              <button type="button" className="btn btn-ghost">
                {SORT_LABEL[sort]}
              </button>
            </MenuTrigger>
            <MenuContent align="end">
              {(Object.keys(SORT_LABEL) as Sort[]).map((s) => (
                <MenuItem key={s} onSelect={() => setSort(s)} icon={sort === s ? '✓' : undefined}>
                  {SORT_LABEL[s]}
                </MenuItem>
              ))}
            </MenuContent>
          </Menu>
          <button type="button" className="btn btn-primary" onClick={() => create()}>
            <Plus size={14} /> Nuova lavagna
          </button>
        </header>

        <div className="home-body">
          <section className="home-section" ref={templatesRef} aria-labelledby="tpl-title">
            <h2 className="home-section-title" id="tpl-title">
              Inizia da un modello
            </h2>
            <div className="templates">
              <button type="button" className="template-card" onClick={() => create()}>
                <TemplateArt template={null} />
                <span className="template-name">Lavagna vuota</span>
                <span className="template-desc">Spazio infinito, tutto da scrivere</span>
              </button>
              {TEMPLATES.map((t) => (
                <button key={t.id} type="button" className="template-card" onClick={() => create(t.id)}>
                  <TemplateArt template={t} />
                  <span className="template-name">{t.name}</span>
                  <span className="template-desc">{t.description}</span>
                </button>
              ))}
            </div>
          </section>

          <section className="home-section" aria-labelledby="boards-title">
            <h2 className="home-section-title" id="boards-title">
              Le tue lavagne
            </h2>
            {boards && !boards.length ? (
              <div className="empty-state">
                <h2>Nessuna lavagna, per ora</h2>
                <p>Crea una lavagna vuota o parti da un modello. Tutto viene salvato su questo PC mentre lavori.</p>
                <button type="button" className="btn btn-primary btn-lg" onClick={() => create()}>
                  <Plus size={14} /> Nuova lavagna
                </button>
              </div>
            ) : boards && !shown.length ? (
              <p className="hint">Nessuna lavagna contiene «{query}».</p>
            ) : (
              <div className="board-grid">
                {shown.map((b) => (
                  <article key={b.id} className="board-card">
                    <button type="button" className="board-thumb" onClick={() => onOpen(b.id)} aria-label={`Apri ${b.title}`}>
                      {b.has_thumb ? (
                        <img src={`/api/boards/${b.id}/thumbnail?v=${b.updated_at}`} alt="" loading="lazy" />
                      ) : (
                        <span className="board-thumb-empty">
                          <PenLine size={24} />
                        </span>
                      )}
                      {sharedBoard === b.id && <span className="live-pill badge-live">In condivisione</span>}
                    </button>
                    <div className="board-meta">
                      <span className="board-icon" aria-hidden="true">
                        <PenLine size={12} />
                      </span>
                      <div className="board-meta-text">
                        {renaming === b.id ? (
                          <input
                            className="rename-input"
                            aria-label="Nuovo nome"
                            defaultValue={b.title}
                            autoFocus
                            onFocus={(e) => e.target.select()}
                            onBlur={(e) => {
                              setRenaming(null)
                              const t = e.target.value.trim()
                              if (t && t !== b.title) act(() => api(`/api/boards/${b.id}`, { method: 'PATCH', json: { title: t } }))
                            }}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                              if (e.key === 'Escape') setRenaming(null)
                            }}
                          />
                        ) : (
                          <div className="board-name" title={b.title}>
                            {b.title}
                          </div>
                        )}
                        <div className="board-sub">Modificata {timeAgo(b.updated_at)}</div>
                      </div>
                      <Menu>
                        <MenuTrigger asChild>
                          <IconButton label="Altre azioni" className="board-more" noTip>
                            <MoreHorizontal size={16} />
                          </IconButton>
                        </MenuTrigger>
                        <MenuContent align="end">
                          <MenuItem icon={<Pencil size={14} />} onSelect={() => setRenaming(b.id)}>
                            Rinomina
                          </MenuItem>
                          <MenuItem icon={<Copy size={14} />} onSelect={() => act(() => api(`/api/boards/${b.id}/duplicate`, { method: 'POST' }), 'Lavagna duplicata.')}>
                            Duplica
                          </MenuItem>
                          <MenuItem
                            icon={<Download size={14} />}
                            onSelect={() =>
                              act(async () => {
                                const data = await api(`/api/boards/${b.id}/export`)
                                download(JSON.stringify(data), `${safeFilename(b.title)}.tratto`, 'application/json')
                              })
                            }
                          >
                            Esporta file .tratto
                          </MenuItem>
                          <MenuSep />
                          <MenuItem icon={<Trash2 size={14} />} danger onSelect={() => setDeleting(b)}>
                            Elimina
                          </MenuItem>
                        </MenuContent>
                      </Menu>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </section>
        </div>
      </main>

      <input
        ref={importInput}
        type="file"
        accept=".tratto,application/json"
        hidden
        onChange={(e) => {
          const f = e.target.files?.[0]
          e.target.value = ''
          if (f) importFile(f)
        }}
      />

      <Dialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title="Eliminare la lavagna?"
        description={deleting ? `«${deleting.title}» e le sue immagini verranno cancellate da questo PC. Non si può annullare.` : undefined}
        footer={
          <>
            <button type="button" className="btn btn-secondary" onClick={() => setDeleting(null)}>
              Annulla
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => {
                const b = deleting!
                setDeleting(null)
                act(() => api(`/api/boards/${b.id}`, { method: 'DELETE' }), 'Lavagna eliminata.')
              }}
            >
              Elimina
            </button>
          </>
        }
      />
    </div>
  )
}
