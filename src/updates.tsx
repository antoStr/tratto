import { useEffect, useState } from 'react'
import { bridge, type UpdateState } from './desktop.ts'
import './updates.css'

export function useUpdateState(): UpdateState {
  const [state, setState] = useState<UpdateState>({ state: bridge ? 'idle' : 'unsupported' } as UpdateState)
  useEffect(() => {
    if (!bridge) return
    let live = true
    void bridge.getUpdateState().then((s) => live && setState(s))
    const off = bridge.onUpdate(setState)
    return () => {
      live = false
      off()
    }
  }, [])
  return state
}

export function UpdateIndicator() {
  const s = useUpdateState()
  if (s.state === 'downloading') {
    return <span className="upd-pill" role="status">Scarico l'aggiornamento… {s.percent}%</span>
  }
  if (s.state === 'ready') {
    return (
      <button type="button" className="upd-pill" aria-label={`Riavvia per aggiornare Tratto alla versione ${s.version}`} onClick={() => bridge?.installUpdate()}>
        Riavvia per aggiornare
      </button>
    )
  }
  return null
}

function statusText(s: UpdateState): string {
  switch (s.state) {
    case 'unsupported':
      return "Gli aggiornamenti automatici funzionano nell'app installata."
    case 'idle':
      return 'Nessun controllo ancora eseguito.'
    case 'checking':
      return 'Controllo in corso…'
    case 'latest':
      return `Hai l'ultima versione · controllato alle ${new Date(s.checkedAt).toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' })}`
    case 'downloading':
      return `Nuova versione ${s.version} in download (${s.percent}%)`
    case 'ready':
      return `Versione ${s.version} pronta: riavvia per installarla`
    case 'error':
      return s.message
  }
}

export function UpdatesSettings() {
  const s = useUpdateState()
  const [version, setVersion] = useState('')
  useEffect(() => {
    void bridge?.getInfo().then((i) => setVersion(i.version))
  }, [])
  const busy = s.state === 'checking' || s.state === 'downloading'
  return (
    <div className="upd-settings">
      {version && <p className="hint">Versione installata: {version}</p>}
      <div className="upd-status">
        {s.state === 'checking' && <span className="spinner" aria-hidden="true" />}
        <p className={s.state === 'error' ? 'hint error-text' : 'hint'} role="status">{statusText(s)}</p>
      </div>
      {s.state !== 'unsupported' && (
        <div className="upd-actions">
          <button type="button" className="btn btn-secondary" disabled={busy || s.state === 'ready'} onClick={() => void bridge?.checkForUpdates()}>
            Cerca aggiornamenti
          </button>
          {s.state === 'ready' && (
            <button type="button" className="btn btn-primary" onClick={() => bridge?.installUpdate()}>
              Riavvia e aggiorna
            </button>
          )}
        </div>
      )}
    </div>
  )
}
