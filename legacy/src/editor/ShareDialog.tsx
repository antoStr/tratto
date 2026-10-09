import { useEffect, useState } from 'react'
import { create } from 'zustand'
import { Check, Copy, Link2, RefreshCw, ShieldCheck } from 'lucide-react'
import { api, type ShareState } from '../api.ts'
import { Avatar, Dialog, Segmented, Switch, toast } from '../ui.tsx'
import { useEditor } from './store.ts'

/* ---------- sharing state (host), refreshed while a share is on ---------- */

export const useShare = create<{ state: ShareState; busy: boolean }>(() => ({ state: { active: false }, busy: false }))

async function call(path: string, method: string, json?: unknown) {
  try {
    const state = await api<ShareState>(path, { method, json })
    useShare.setState({ state })
    return state
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), 'error')
    return null
  }
}

export const share = {
  refresh: () => call('/api/share', 'GET'),
  start: async (boardId: string, access: 'edit' | 'view', autoAdmit: boolean) => {
    useShare.setState({ busy: true })
    const s = await call('/api/share', 'POST', { boardId, access, autoAdmit })
    useShare.setState({ busy: false })
    return s
  },
  update: (patch: { access?: 'edit' | 'view'; autoAdmit?: boolean; rotate?: boolean }) => call('/api/share', 'PATCH', patch),
  admit: (ticket: string, allow: boolean) => call('/api/share/admit', 'POST', { ticket, allow }),
  kick: (ticket: string) => call('/api/share/kick', 'POST', { ticket }),
  stop: () => call('/api/share', 'DELETE'),
}

/** Polls the share state while sharing is on, so join requests show up quickly. */
export function useSharePolling() {
  const active = useShare((s) => s.state.active)
  useEffect(() => {
    share.refresh()
  }, [])
  useEffect(() => {
    if (!active) return
    const t = setInterval(share.refresh, 1500)
    return () => clearInterval(t)
  }, [active])
}

/* ---------- dialog ---------- */

export function ShareDialog({ open, onOpenChange, boardId, title }: { open: boolean; onOpenChange: (o: boolean) => void; boardId: string; title: string }) {
  const { state, busy } = useShare()
  const prefs = useEditor((s) => s.prefs)
  const [access, setAccess] = useState<'edit' | 'view'>('edit')
  const [autoAdmit, setAutoAdmit] = useState(false)
  const [copied, setCopied] = useState(false)
  const here = state.active && state.boardId === boardId
  const elsewhere = state.active && state.boardId !== boardId

  const copy = async () => {
    if (!state.url) return
    try {
      await navigator.clipboard.writeText(state.url)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      toast('Non riesco a copiare: seleziona il link e premi Ctrl+C.', 'error')
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      wide
      title={`Condividi «${title}»`}
      footer={
        here && state.status !== 'starting' ? (
          <>
            <button type="button" className="btn btn-secondary" onClick={() => share.update({ rotate: true }).then((s) => s && toast('Nuovo link creato. Quello vecchio non funziona più.'))}>
              <RefreshCw size={12} />
              Nuovo link
            </button>
            <div className="grow" />
            <button type="button" className="btn btn-secondary danger-text" onClick={() => share.stop().then((s) => s && toast('Condivisione terminata. Il link non funziona più.'))}>
              Termina condivisione
            </button>
          </>
        ) : undefined
      }
    >
      {!here && (
        <>
          <p className="dialog-desc">Invita qualcuno a lavorare con te su questa lavagna. Riceverà un link da aprire nel browser (Windows, Mac, Linux, tablet), senza installare niente. Possono esserci fino a 30 persone insieme.</p>
          {elsewhere && <p className="share-status">Stai già condividendo un'altra lavagna. Se condividi questa, l'altra condivisione si chiude.</p>}
          <div className="form-row">
            <label className="label" htmlFor="share-name">
              Il tuo nome
            </label>
            <div className="field lg">
              <input id="share-name" value={prefs.name} maxLength={40} placeholder="Come ti vedranno gli altri" onChange={(e) => useEditor.getState().setPrefs({ name: e.target.value })} autoComplete="name" />
            </div>
          </div>
          <div className="form-row">
            <span className="label" id="share-access">
              Chi ha il link
            </span>
            <Segmented
              label="Chi ha il link"
              value={access}
              onChange={setAccess}
              options={[
                { value: 'edit', label: 'Può modificare' },
                { value: 'view', label: 'Può solo guardare' },
              ]}
            />
          </div>
          <Switch id="share-auto" label="Fai entrare senza chiedermelo" checked={autoAdmit} onChange={setAutoAdmit} />
          <button type="button" className="btn btn-primary btn-lg btn-block" style={{ marginTop: 16 }} disabled={busy} onClick={() => share.start(boardId, access, autoAdmit)}>
            {busy ? (
              <>
                <span className="spinner" aria-hidden="true" /> Apro un collegamento sicuro…
              </>
            ) : (
              <>
                <Link2 size={14} /> Crea link di invito
              </>
            )}
          </button>
          <SecureNote />
        </>
      )}

      {here && state.status === 'error' && (
        <>
          <p className="share-status error" role="alert">
            {state.error ?? 'La condivisione non è partita.'}
          </p>
          <button type="button" className="btn btn-primary btn-lg btn-block" style={{ marginTop: 12 }} disabled={busy} onClick={() => share.start(boardId, state.access ?? 'edit', !!state.autoAdmit)}>
            {busy ? <span className="spinner" aria-hidden="true" /> : <RefreshCw size={14} />}
            Riprova
          </button>
        </>
      )}

      {here && state.status === 'live' && state.url && (
        <>
          <div className="form-row">
            <label className="label" htmlFor="share-url">
              Link di invito
            </label>
            <div className="share-link">
              <div className="field lg">
                <input id="share-url" readOnly value={state.url} onFocus={(e) => e.target.select()} />
              </div>
              <button type="button" className="btn btn-primary btn-lg" onClick={copy}>
                {copied ? <Check size={14} /> : <Copy size={14} />}
                {copied ? 'Copiato' : 'Copia link'}
              </button>
            </div>
          </div>
          <div className="form-row">
            <span className="label">Chi ha il link</span>
            <Segmented
              label="Chi ha il link"
              value={state.access ?? 'edit'}
              onChange={(a) => share.update({ access: a })}
              options={[
                { value: 'edit', label: 'Può modificare' },
                { value: 'view', label: 'Può solo guardare' },
              ]}
            />
          </div>
          <Switch id="share-auto-live" label="Fai entrare senza chiedermelo" checked={!!state.autoAdmit} onChange={(v) => share.update({ autoAdmit: v })} />

          {!!state.requests?.length && (
            <div className="people" aria-label="Richieste di accesso">
              {state.requests.map((r) => (
                <div key={r.ticket} className="request">
                  <Avatar name={r.name} color="#9747FF" />
                  <span className="person-name">
                    <strong>{r.name}</strong> vuole entrare
                  </span>
                  <button type="button" className="btn btn-secondary" onClick={() => share.admit(r.ticket, false)}>
                    Rifiuta
                  </button>
                  <button type="button" className="btn btn-primary" onClick={() => share.admit(r.ticket, true)}>
                    Fai entrare
                  </button>
                </div>
              ))}
            </div>
          )}

          <div className="people" aria-label="Persone">
            <div className="person">
              <Avatar name={prefs.name || 'Tu'} color="#0D99FF" />
              <span className="person-name">{prefs.name || 'Tu'} (tu)</span>
              <span className="person-role">Proprietario</span>
            </div>
            {state.guests?.map((g) => (
              <div key={g.ticket} className="person">
                <Avatar name={g.name} color="#9747FF" />
                <span className="person-name">{g.name}</span>
                <span className="person-role">{g.online ? (state.access === 'edit' ? 'Può modificare' : 'Guarda') : 'Non collegato'}</span>
                <button type="button" className="btn btn-ghost danger-text" onClick={() => share.kick(g.ticket)}>
                  Rimuovi
                </button>
              </div>
            ))}
          </div>
          <SecureNote />
        </>
      )}
    </Dialog>
  )
}

function SecureNote() {
  return (
    <p className="secure-note">
      <ShieldCheck size={14} aria-hidden="true" />
      <span>Il collegamento passa da un tunnel cifrato di Cloudflare: il tuo indirizzo IP resta nascosto e non devi aprire porte sul router. Quando termini la condivisione o chiudi Tratto, il link smette di funzionare.</span>
    </p>
  )
}

/** Join requests shown over the canvas while the share dialog is closed. */
export function JoinRequests() {
  const requests = useShare((s) => s.state.requests)
  if (!requests?.length) return null
  return (
    <div className="knock" role="region" aria-label="Richieste di accesso" aria-live="polite">
      {requests.slice(0, 3).map((r) => (
        <div key={r.ticket} className="toast">
          <span>
            <strong>{r.name}</strong> vuole entrare nella lavagna
          </span>
          <button type="button" style={{ color: 'var(--menu-text-2)' }} onClick={() => share.admit(r.ticket, false)}>
            Rifiuta
          </button>
          <button type="button" onClick={() => share.admit(r.ticket, true)}>
            Fai entrare
          </button>
        </div>
      ))}
    </div>
  )
}
