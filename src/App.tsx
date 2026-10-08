import { useEffect, useState, type FormEvent } from 'react'
import { Tooltip } from 'radix-ui'
import { Home as HomeIcon, ShieldCheck } from 'lucide-react'
import { api, ApiError, setGuestCode } from './api.ts'
import { bridge, isDesktop } from './desktop.ts'
import { Editor, type GuestSession } from './editor/Editor.tsx'
import { useShare } from './editor/ShareDialog.tsx'
import { useEditor } from './editor/store.ts'
import { Home } from './home/Home.tsx'
import { Logo, Toaster, toast } from './ui.tsx'

if (isDesktop) document.documentElement.classList.add('desktop')

function useTheme() {
  const theme = useEditor((s) => s.prefs.theme)
  useEffect(() => {
    bridge?.setTheme(theme)
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = () => {
      document.documentElement.dataset.theme = theme === 'system' ? (mq.matches ? 'dark' : 'light') : theme
    }
    apply()
    mq.addEventListener('change', apply)
    return () => mq.removeEventListener('change', apply)
  }, [theme])
}

/** The session code travels after `#`, so it never reaches any server log. */
const guestCode = /^#[A-Za-z0-9_-]{16,64}$/.test(location.hash) ? location.hash.slice(1) : null

export function App() {
  useTheme()
  return (
    <Tooltip.Provider delayDuration={600} skipDelayDuration={250}>
      {guestCode ? <GuestApp code={guestCode} /> : <HostApp />}
      <Toaster />
    </Tooltip.Provider>
  )
}

/* ---------------- host (the app on this PC) ---------------- */

function parseRoute() {
  const m = /^\/b\/([A-Za-z0-9_-]{6,64})$/.exec(location.pathname)
  return m ? { boardId: m[1], template: new URLSearchParams(location.search).get('template') } : null
}

function HostApp() {
  const [route, setRoute] = useState(parseRoute)
  const [title, setTitle] = useState<string | null>(null)
  const live = useShare((s) => s.state.active && s.state.status === 'live')

  useEffect(() => {
    const onPop = () => setRoute(parseRoute())
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  useEffect(() => {
    setTitle(null)
    if (!route) return
    api<{ title: string }>(`/api/boards/${route.boardId}`)
      .then((b) => setTitle(b.title))
      .catch((e) => {
        toast(e instanceof ApiError && e.status === 404 ? 'Questa lavagna non esiste più.' : e.message, 'error')
        go('/')
      })
  }, [route?.boardId])

  const go = (path: string) => {
    history.pushState(null, '', path)
    setRoute(parseRoute())
  }
  const open = (id: string, template?: string) => go(`/b/${id}${template ? `?template=${template}` : ''}`)
  const newBoard = async () => {
    try {
      const { id } = await api<{ id: string }>('/api/boards', { method: 'POST', json: { title: 'Lavagna senza titolo' } })
      open(id)
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  return (
    <>
      <header className="titlebar">
        <button type="button" className="tb-home" aria-label="Lavagne" aria-current={!route ? 'page' : undefined} onClick={() => go('/')}>
          <HomeIcon size={16} />
        </button>
        {route && (
          <button type="button" className="tab" aria-current="page">
            <Logo size={14} />
            <span>{title ?? 'Caricamento…'}</span>
          </button>
        )}
        <div className="grow" />
        {live && <span className="live-pill">In condivisione</span>}
      </header>
      {route ? (
        title !== null && <Editor key={route.boardId} boardId={route.boardId} title={title} template={route.template} onTitle={setTitle} onHome={() => go('/')} onNewBoard={newBoard} />
      ) : (
        <Home onOpen={open} />
      )}
    </>
  )
}

/* ---------------- guest (opened from a shared link) ---------------- */

type GuestStep =
  | { step: 'checking' }
  | { step: 'invalid'; message: string }
  | { step: 'name'; title: string; boardId: string; access: 'edit' | 'view' }
  | { step: 'waiting'; title: string; boardId: string; access: 'edit' | 'view'; ticket: string; name: string }
  | { step: 'denied' }
  | { step: 'in'; title: string; boardId: string; session: GuestSession }
  | { step: 'ended'; reason: 'ended' | 'removed' }

const NAME_KEY = 'tratto.guestName'

function GuestApp({ code }: { code: string }) {
  const [s, setS] = useState<GuestStep>({ step: 'checking' })

  useEffect(() => {
    setGuestCode(code)
    api<{ title: string; boardId: string; access: 'edit' | 'view' }>('/api/guest/session')
      .then((r) => setS({ step: 'name', ...r }))
      .catch((e: Error) => setS({ step: 'invalid', message: e.message }))
  }, [code])

  // While waiting to be let in, check every 1.5 s.
  useEffect(() => {
    if (s.step !== 'waiting') return
    const t = setInterval(async () => {
      try {
        const r = await api<{ status: string }>('/api/guest/ticket', { headers: { Authorization: `Bearer ${code}.${s.ticket}` } })
        if (r.status === 'approved') enter(s, s.ticket, s.name)
        if (r.status === 'denied') setS({ step: 'denied' })
      } catch (e) {
        setS({ step: 'invalid', message: (e as Error).message })
      }
    }, 1500)
    return () => clearInterval(t)
  }, [s, code])

  const enter = (info: { title: string; boardId: string; access: 'edit' | 'view' }, ticket: string, name: string) => {
    setGuestCode(`${code}.${ticket}`)
    setS({ step: 'in', title: info.title, boardId: info.boardId, session: { code, ticket, name, access: info.access } })
  }

  if (s.step === 'in')
    return <Editor key={s.session.ticket} boardId={s.boardId} title={s.title} guest={s.session} onEnded={(reason) => setS({ step: 'ended', reason })} />

  return (
    <div className="screen">
      <div className="card" role="main">
        <Logo size={32} />
        {s.step === 'checking' && (
          <>
            <h1>Un attimo…</h1>
            <p>Sto aprendo la lavagna condivisa.</p>
          </>
        )}
        {s.step === 'invalid' && (
          <>
            <h1>Il link non funziona</h1>
            <p>{s.message} Chiedi a chi ti ha invitato di mandarti un link nuovo.</p>
          </>
        )}
        {s.step === 'name' && <JoinForm title={s.title} onJoin={(name) => join(name)} />}
        {s.step === 'waiting' && (
          <>
            <h1>Aspetta che ti facciano entrare</h1>
            <p>Abbiamo avvisato chi ha condiviso «{s.title}». Entrerai appena accetta.</p>
            <span className="spinner" aria-label="In attesa" />
          </>
        )}
        {s.step === 'denied' && (
          <>
            <h1>Non sei stato fatto entrare</h1>
            <p>Chi ha condiviso la lavagna non ha accettato la richiesta. Se pensi sia un errore, contattalo.</p>
          </>
        )}
        {s.step === 'ended' && (
          <>
            <h1>{s.reason === 'removed' ? 'Non sei più nella lavagna' : 'La condivisione è terminata'}</h1>
            <p>{s.reason === 'removed' ? 'Chi ha condiviso la lavagna ti ha tolto l’accesso o ha creato un link nuovo.' : 'Chi ha condiviso la lavagna l’ha chiusa. Per rientrare serve un nuovo invito.'}</p>
            <button type="button" className="btn btn-secondary btn-lg" onClick={() => location.reload()}>
              Riprova
            </button>
          </>
        )}
        <p className="privacy-note">
          <ShieldCheck size={14} aria-hidden="true" />
          <span>Collegamento cifrato. Gli altri partecipanti non vedono il tuo indirizzo IP.</span>
        </p>
      </div>
    </div>
  )

  async function join(name: string) {
    if (s.step !== 'name') return
    try {
      localStorage.setItem(NAME_KEY, name)
    } catch {
      /* ignore */
    }
    try {
      const r = await api<{ ticket: string; status: 'pending' | 'approved' }>('/api/guest/join', { method: 'POST', json: { name } })
      if (r.status === 'approved') enter(s, r.ticket, name)
      else setS({ ...s, step: 'waiting', ticket: r.ticket, name })
    } catch (e) {
      toast((e as Error).message, 'error')
    }
  }
}

function JoinForm({ title, onJoin }: { title: string; onJoin: (name: string) => void }) {
  const [name, setName] = useState(() => {
    try {
      return localStorage.getItem(NAME_KEY) ?? ''
    } catch {
      return ''
    }
  })
  const [busy, setBusy] = useState(false)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setBusy(true)
    onJoin(name.trim())
  }
  return (
    <>
      <h1>Entra in «{title}»</h1>
      <p>Scrivi il tuo nome: gli altri lo vedranno accanto al tuo cursore.</p>
      <form onSubmit={submit}>
        <div>
          <label className="label" htmlFor="guest-name">
            Il tuo nome
          </label>
          <div className="field lg">
            <input id="guest-name" value={name} maxLength={40} required autoFocus autoComplete="name" onChange={(e) => setName(e.target.value)} />
          </div>
        </div>
        <button type="submit" className="btn btn-primary btn-lg btn-block" disabled={!name.trim() || busy}>
          {busy ? <span className="spinner" aria-hidden="true" /> : null}
          Chiedi di entrare
        </button>
      </form>
    </>
  )
}
