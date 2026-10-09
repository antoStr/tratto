import { useEffect, useRef, useState } from 'react'
import { Popover } from 'radix-ui'
import { Check, Pause, Play, Presentation, Square, Timer as TimerIcon, Trash2, Vote, X } from 'lucide-react'
import type { Board } from './doc.ts'
import { commands, type Awareness } from './controller.ts'
import { toScreen } from './geometry.ts'
import { peerColor, peerName, useBoardVersion, usePeers } from './overlays.tsx'
import { useEditor, voterId } from './store.ts'
import { Avatar, IconButton, Segmented } from '../ui.tsx'
import { COMMENT_PIN, isDark } from './render.ts'
import { elementLabel } from './Inspector.tsx'

/* ---------------- shared timer (FigJam) ---------------- */

/** `end`: when it reaches zero (setter's clock), null while paused; `left`: ms left when paused. */
interface TimerState {
  total: number
  end: number | null
  left: number
}

const num = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)

function readTimer(board: Board): TimerState | null {
  const total = board.timer.get('total')
  const end = board.timer.get('end')
  const left = board.timer.get('left')
  if (!num(total) || total <= 0 || !num(left)) return null
  return { total: Math.min(total, 24 * 3600_000), end: num(end) ? end : null, left }
}

// Plain transactions: the timer is not part of the undo history.
const timer = {
  start: (board: Board, ms: number) =>
    board.doc.transact(() => {
      board.timer.set('total', ms)
      board.timer.set('left', ms)
      board.timer.set('end', Date.now() + ms)
    }),
  pause: (board: Board, t: TimerState) =>
    board.doc.transact(() => {
      board.timer.set('left', Math.max(0, (t.end ?? Date.now()) - Date.now()))
      board.timer.set('end', null)
    }),
  resume: (board: Board, t: TimerState) => board.timer.set('end', Date.now() + t.left),
  add: (board: Board, t: TimerState, ms: number) =>
    board.doc.transact(() => {
      board.timer.set('total', t.total + ms)
      if (t.end) board.timer.set('end', Math.max(t.end, Date.now()) + ms)
      else board.timer.set('left', t.left + ms)
    }),
  stop: (board: Board) => board.doc.transact(() => board.timer.clear()),
}

const clock = (ms: number) => {
  const s = Math.ceil(ms / 1000)
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

/** Three soft beeps when time is up. */
function beep() {
  try {
    const ctx = new AudioContext()
    for (let i = 0; i < 3; i++) {
      const o = ctx.createOscillator()
      const g = ctx.createGain()
      o.frequency.value = 880
      const t = ctx.currentTime + i * 0.28
      g.gain.setValueAtTime(0.0001, t)
      g.gain.exponentialRampToValueAtTime(0.2, t + 0.02)
      g.gain.exponentialRampToValueAtTime(0.0001, t + 0.22)
      o.connect(g).connect(ctx.destination)
      o.start(t)
      o.stop(t + 0.24)
    }
    setTimeout(() => ctx.close(), 1200)
  } catch {
    /* no audio: the bar still says it */
  }
}

const PRESETS = [1, 3, 5, 10, 15, 30]

/** Toolbar button: start a countdown everyone sees. */
export function TimerButton({ board }: { board: Board }) {
  useBoardVersion(board)
  const readOnly = useEditor((s) => s.readOnly)
  const t = readTimer(board)
  if (readOnly) return null
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <IconButton label="Timer" on={!!t} tipSide="bottom">
          <TimerIcon size={16} />
        </IconButton>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content className="popover timer-pop" side="bottom" align="end" sideOffset={8} collisionPadding={12}>
          <div className="pop-title">Timer per tutti</div>
          <div className="timer-presets">
            {PRESETS.map((m) => (
              <Popover.Close key={m} asChild>
                <button type="button" className="btn btn-secondary" onClick={() => timer.start(board, m * 60_000)}>
                  {m} min
                </button>
              </Popover.Close>
            ))}
          </div>
          <p className="hint">Il conto alla rovescia compare in alto per chiunque sia sulla lavagna.</p>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

/** The running timer, top centre of the board. */
export function TimerBar({ board }: { board: Board }) {
  useBoardVersion(board)
  const readOnly = useEditor((s) => s.readOnly)
  const t = readTimer(board)
  const [, tick] = useState(0)
  const rang = useRef<number | null>(null)
  const running = !!t?.end
  useEffect(() => {
    if (!running) return
    const id = setInterval(() => tick((n) => n + 1), 250)
    return () => clearInterval(id)
  }, [running])
  const left = t ? (t.end ? Math.max(0, t.end - Date.now()) : t.left) : 0
  const done = !!t && left <= 0
  useEffect(() => {
    if (done && t?.end && rang.current !== t.end) {
      rang.current = t.end
      beep()
    }
  }, [done, t?.end])
  if (!t) return null
  return (
    <div className="timer-bar" data-done={done || undefined} role="timer" aria-live="off" aria-label={done ? 'Tempo scaduto' : `Timer: ${clock(left)}`}>
      <TimerIcon size={14} aria-hidden="true" />
      <span className="timer-time num">{done ? 'Tempo scaduto' : clock(left)}</span>
      <span className="timer-track" aria-hidden="true">
        <span style={{ width: `${(left / t.total) * 100}%` }} />
      </span>
      {!readOnly && (
        <>
          {!done &&
            (t.end ? (
              <IconButton label="Pausa" tipSide="bottom" onClick={() => timer.pause(board, t)}>
                <Pause size={14} />
              </IconButton>
            ) : (
              <IconButton label="Riprendi" tipSide="bottom" onClick={() => timer.resume(board, t)}>
                <Play size={14} />
              </IconButton>
            ))}
          <button type="button" className="btn btn-ghost timer-add" onClick={() => timer.add(board, t, 60_000)}>
            +1 min
          </button>
          <IconButton label="Ferma il timer" tipSide="bottom" onClick={() => timer.stop(board)}>
            <Square size={12} />
          </IconButton>
        </>
      )}
    </div>
  )
}

/* ---------------- spotlight: everyone follows the presenter ---------------- */

const spotlightOf = (s: { spotlight?: unknown } | undefined) => (num(s?.spotlight) ? s.spotlight : null)

/** Toolbar button: present (everyone follows your view) or stop. */
export function SpotlightButton({ awareness }: { awareness: Awareness | null }) {
  usePeers(awareness)
  if (!awareness) return null
  const on = spotlightOf(awareness.getLocalState() ?? undefined) !== null
  return (
    <IconButton label={on ? 'Smetti di presentare' : 'Presenta: tutti seguono la tua vista'} on={on} tipSide="bottom" onClick={() => awareness.setLocalStateField('spotlight', on ? null : Date.now())}>
      <Presentation size={16} />
    </IconButton>
  )
}

/**
 * Follows whoever presents (the latest to start), until you move away yourself. Shows who is
 * presenting, and a coloured frame around the board while you follow someone (FigJam).
 */
export function SpotlightBar({ awareness }: { awareness: Awareness | null }) {
  const peers = usePeers(awareness)
  const following = useEditor((s) => s.following)
  const mine = spotlightOf(awareness?.getLocalState() ?? undefined) !== null
  const presenter = peers
    .filter((p) => spotlightOf(p.state) !== null)
    .sort((a, b) => spotlightOf(b.state)! - spotlightOf(a.state)!)[0]
  const key = presenter ? `${presenter.id}:${spotlightOf(presenter.state)}` : null
  const dismissed = useRef<string | null>(null)
  const auto = useRef<number | null>(null)

  useEffect(() => {
    if (!presenter || mine) {
      // The presentation ended: stop following the presenter if we were only following for it.
      if (auto.current !== null && useEditor.getState().following === auto.current) commands.follow(null)
      auto.current = null
      return
    }
    if (dismissed.current === key) return
    const id = presenter.id
    auto.current = id
    commands.follow(id)
    return useEditor.subscribe((s, prev) => {
      if (prev.following === id && s.following !== id) dismissed.current = key
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, mine])

  const followed = following !== null ? peers.find((p) => p.id === following) : undefined
  const color = followed ? peerColor(followed.state) : null
  return (
    <>
      {color && (
        <div className="follow-frame" style={{ ['--follow' as string]: color }} aria-hidden="true">
          <span style={{ background: color, color: isDark(color) ? '#fff' : '#1E1E1E' }}>Stai seguendo {peerName(followed!.state)}</span>
        </div>
      )}
      {mine ? (
        <div className="present-pill" role="status">
          <Presentation size={14} aria-hidden="true" />
          <span>Stai presentando: tutti seguono la tua vista</span>
          <button type="button" className="btn btn-secondary" onClick={() => awareness?.setLocalStateField('spotlight', null)}>
            Interrompi
          </button>
        </div>
      ) : (
        presenter && (
          <div className="present-pill" role="status">
            <Presentation size={14} aria-hidden="true" />
            <span>{peerName(presenter.state)} sta presentando</span>
            {following === presenter.id ? (
              <button type="button" className="btn btn-secondary" onClick={() => commands.follow(null)}>
                Smetti di seguire
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  dismissed.current = null
                  commands.follow(presenter.id)
                }}
              >
                Segui
              </button>
            )}
          </div>
        )
      )}
    </>
  )
}

/* ---------------- cursor chat: press / and type, others see it by your cursor ---------------- */

export function CursorChat({ awareness }: { awareness: Awareness | null }) {
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const [at, setAt] = useState({ x: 0, y: 0 })
  const clearTimer = useRef(0)
  const last = useRef({ x: window.innerWidth / 2, y: window.innerHeight / 2 })

  useEffect(() => {
    const move = (e: PointerEvent) => {
      last.current = { x: e.clientX, y: e.clientY }
      if (open) setAt(last.current)
    }
    const start = () => {
      clearTimeout(clearTimer.current)
      setText('')
      setAt(last.current)
      setOpen(true)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('tratto:chat', start)
    return () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('tratto:chat', start)
    }
  }, [open])

  if (!open || !awareness) return null
  const send = (t: string) => awareness.setLocalStateField('chat', t.trim() ? t.slice(0, 80) : null)
  // The message stays a few seconds after you finish, then fades for everyone.
  const close = (keep: boolean) => {
    setOpen(false)
    clearTimeout(clearTimer.current)
    if (keep) clearTimer.current = window.setTimeout(() => send(''), 5000)
    else send('')
  }
  const color = peerColor(awareness.getLocalState() ?? undefined)
  return (
    <div className="chat-bubble" style={{ left: at.x + 14, top: at.y + 18, background: color, color: isDark(color) ? '#fff' : '#1E1E1E' }}>
      <input
        autoFocus
        aria-label="Messaggio vicino al cursore, lo vedono tutti"
        placeholder="Scrivi un messaggio…"
        maxLength={80}
        value={text}
        onChange={(e) => {
          setText(e.target.value)
          send(e.target.value)
        }}
        onKeyDown={(e) => {
          e.stopPropagation()
          if (e.key === 'Enter') close(true)
          if (e.key === 'Escape') close(false)
        }}
        onBlur={() => close(true)}
      />
    </div>
  )
}

/* ---------------- voting (FigJam) ---------------- */

/** Toolbar button: start a vote, choosing how many votes each person has. */
export function VoteButton({ board }: { board: Board }) {
  useBoardVersion(board)
  const readOnly = useEditor((s) => s.readOnly)
  const [per, setPer] = useState('3')
  if (readOnly) return null
  const v = board.voting()
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <IconButton label="Votazione" on={!!v} tipSide="bottom">
          <Vote size={16} />
        </IconButton>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content className="popover timer-pop" side="bottom" align="end" sideOffset={8} collisionPadding={12}>
          <div className="pop-title">Votazione</div>
          <p className="hint">Ognuno clicca sulle note, forme o immagini che preferisce. I voti degli altri si vedono alla fine.</p>
          <div className="pop-title">Voti per persona</div>
          <Segmented<string>
            label="Voti per persona"
            value={per}
            onChange={setPer}
            options={['1', '3', '5', '10'].map((n) => ({ value: n, label: n }))}
          />
          <Popover.Close asChild>
            <button
              type="button"
              className="btn btn-primary btn-block vote-start"
              onClick={() => {
                board.startVoting(Number(per))
                useEditor.getState().setTool('select')
              }}
            >
              {v ? 'Ricomincia la votazione' : 'Inizia la votazione'}
            </button>
          </Popover.Close>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

/** Top of the board while voting: votes left, then the results. */
export function VoteBar({ board }: { board: Board }) {
  useBoardVersion(board)
  const readOnly = useEditor((s) => s.readOnly)
  const v = board.voting()
  if (!v) return null
  if (!v.ended) {
    const used = [...board.tally(voterId()).values()].reduce((a, b) => a + b, 0)
    return (
      <div className="present-pill" role="status">
        <Vote size={14} aria-hidden="true" />
        <span>
          Votazione: {readOnly ? 'solo chi può modificare vota' : `ti restano ${v.per - used} voti su ${v.per}. Clicca per votare, Maiusc+clic per togliere`}
        </span>
        {!readOnly && (
          <button type="button" className="btn btn-secondary" onClick={() => board.endVoting()}>
            Termina
          </button>
        )}
      </div>
    )
  }
  const results = [...board.tally()].sort((a, b) => b[1] - a[1]).slice(0, 3)
  return (
    <div className="present-pill" role="status">
      <Vote size={14} aria-hidden="true" />
      <span>{results.length ? 'Più votati:' : 'Nessun voto.'}</span>
      {results.map(([id, n]) => {
        const el = board.get(id)
        return (
          el && (
            <button key={id} type="button" className="btn btn-ghost vote-result" onClick={() => commands.reveal([id])}>
              {elementLabel(el).slice(0, 18)} · {n}
            </button>
          )
        )
      })}
      {!readOnly && (
        <button type="button" className="btn btn-secondary" onClick={() => board.clearVoting()}>
          Chiudi
        </button>
      )}
    </div>
  )
}

/* ---------------- comments ---------------- */

const when = (t: number) => {
  const s = (Date.now() - t) / 1000
  const rtf = new Intl.RelativeTimeFormat('it', { numeric: 'auto' })
  if (s < 60) return 'adesso'
  if (s < 3600) return rtf.format(-Math.round(s / 60), 'minute')
  if (s < 86400) return rtf.format(-Math.round(s / 3600), 'hour')
  return rtf.format(-Math.round(s / 86400), 'day')
}

/** The open comment's thread, beside its pin: messages, reply, resolve. */
export function CommentThread({ board, awareness }: { board: Board; awareness: Awareness | null }) {
  useBoardVersion(board)
  const id = useEditor((s) => s.commentId)
  const cam = useEditor((s) => s.camera)
  const readOnly = useEditor((s) => s.readOnly)
  const [draft, setDraft] = useState('')
  const el = id ? board.get(id) : undefined

  useEffect(() => {
    if (id && !el) useEditor.setState({ commentId: null })
  }, [id, el])
  // Closing a new comment without writing anything takes the pin away again.
  useEffect(() => {
    if (!id) return
    setDraft('')
    return () => {
      const c = board.get(id)
      if (c?.type === 'comment' && !c.thread.length) board.remove([id])
    }
  }, [id, board])

  if (!el || el.type !== 'comment') return null
  const close = () => useEditor.setState({ commentId: null })
  const p = toScreen(cam, el.x, el.y)
  const send = () => {
    const text = draft.trim()
    if (!text) return
    const me = awareness?.getLocalState() ?? undefined
    board.update(el.id, { thread: [...el.thread, { author: peerName(me), color: peerColor(me), text: text.slice(0, 2000), t: Date.now() }] })
    setDraft('')
  }
  return (
    <div
      className="comment-thread"
      role="dialog"
      aria-label="Commento"
      style={{ left: `max(8px, min(${p.x + COMMENT_PIN + 10}px, calc(100% - 300px)))`, top: `max(8px, min(${p.y - COMMENT_PIN}px, calc(100% - 320px)))` }}
      onKeyDown={(e) => {
        e.stopPropagation()
        if (e.key === 'Escape') close()
      }}
    >
      <div className="ct-head">
        <strong>Commento</strong>
        <div className="grow" />
        {!readOnly && el.thread.length > 0 && (
          <IconButton
            label="Risolvi: togli il commento"
            onClick={() => {
              board.remove([el.id])
              close()
            }}
          >
            <Check size={14} />
          </IconButton>
        )}
        <IconButton label="Chiudi" onClick={close}>
          <X size={14} />
        </IconButton>
      </div>
      {el.thread.length > 0 && (
        <div className="ct-list">
          {el.thread.map((m, i) => (
            <div key={i} className="ct-msg">
              <Avatar name={m.author} color={peerColor({ user: { color: m.color } })} />
              <div>
                <div className="ct-meta">
                  <strong>{m.author}</strong>
                  <span>{when(m.t)}</span>
                  {!readOnly && i > 0 && (
                    <IconButton label="Elimina la risposta" size="sm" className="ct-del" onClick={() => board.update(el.id, { thread: el.thread.filter((_, k) => k !== i) })}>
                      <Trash2 size={12} />
                    </IconButton>
                  )}
                </div>
                <p>{m.text}</p>
              </div>
            </div>
          ))}
        </div>
      )}
      {!readOnly && (
        <div className="ct-reply">
          <textarea
            autoFocus
            rows={2}
            aria-label={el.thread.length ? 'Rispondi' : 'Scrivi un commento'}
            placeholder={el.thread.length ? 'Rispondi…' : 'Scrivi un commento…'}
            value={draft}
            maxLength={2000}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                send()
              }
            }}
          />
          <button type="button" className="btn btn-primary" disabled={!draft.trim()} onClick={send}>
            Invia
          </button>
        </div>
      )}
    </div>
  )
}
