import * as Y from 'yjs'
import { DEFAULT_META, type BoardMeta, type El } from './types.ts'

/** Transaction origin for this user's edits: the undo manager only tracks these. */
export const LOCAL = { local: true }

export const uid = () => crypto.getRandomValues(new Uint32Array(3)).reduce((s, n) => s + n.toString(36).padStart(7, '0'), '')

/**
 * The shared board. Every element is a plain JSON object in one Y.Map, replaced as a
 * whole on edit: concurrent edits to the same element resolve last-writer-wins.
 */
export class Board {
  readonly doc: Y.Doc
  readonly elements: Y.Map<El>
  readonly meta: Y.Map<unknown>
  readonly undo: Y.UndoManager
  private sorted: El[] | null = null
  private listeners = new Set<() => void>()
  version = 0

  constructor(doc: Y.Doc) {
    this.doc = doc
    this.elements = doc.getMap<El>('elements')
    this.meta = doc.getMap('meta')
    // One undo step per gesture: callers invoke undo.stopCapturing() when a gesture starts.
    this.undo = new Y.UndoManager([this.elements, this.meta], { trackedOrigins: new Set([LOCAL]), captureTimeout: 60_000 })
    const changed = () => {
      this.sorted = null
      this.version++
      for (const l of this.listeners) l()
    }
    this.elements.observe(changed)
    this.meta.observe(changed)
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn)
    return () => this.listeners.delete(fn)
  }

  /** Elements bottom to top. */
  all(): El[] {
    if (!this.sorted) {
      this.sorted = [...this.elements.values()].filter(isValid).sort((a, b) => a.z - b.z || (a.id < b.id ? -1 : 1))
    }
    return this.sorted
  }

  get(id: string): El | undefined {
    const el = this.elements.get(id)
    return el && isValid(el) ? el : undefined
  }

  getMeta(): BoardMeta {
    const bg = this.meta.get('background')
    const pattern = this.meta.get('pattern')
    return {
      background: typeof bg === 'string' && /^#[0-9a-f]{6}$/i.test(bg) ? bg : DEFAULT_META.background,
      pattern: pattern === 'none' || pattern === 'dots' || pattern === 'grid' || pattern === 'lines' ? pattern : DEFAULT_META.pattern,
    }
  }

  private lastLocal = 0

  /** Local edit. Edits after a pause of more than 0.8 s start a new undo step on their own. */
  transact(fn: () => void) {
    const now = Date.now()
    if (now - this.lastLocal > 800) this.undo.stopCapturing()
    this.lastLocal = now
    this.doc.transact(fn, LOCAL)
  }

  topZ() {
    const all = this.all()
    return all.length ? all[all.length - 1].z + 1 : 0
  }

  bottomZ() {
    const all = this.all()
    return all.length ? all[0].z - 1 : 0
  }

  add(els: El[]) {
    this.transact(() => {
      for (const el of els) this.elements.set(el.id, el)
    })
  }

  update(id: string, patch: Partial<El>) {
    const el = this.elements.get(id)
    if (el) this.transact(() => this.elements.set(id, { ...el, ...patch } as El))
  }

  updateMany(patches: Map<string, Partial<El>>) {
    this.transact(() => {
      for (const [id, patch] of patches) {
        const el = this.elements.get(id)
        if (el) this.elements.set(id, { ...el, ...patch } as El)
      }
    })
  }

  remove(ids: Iterable<string>) {
    this.transact(() => {
      for (const id of ids) this.elements.delete(id)
    })
  }

  setMeta(patch: Partial<BoardMeta>) {
    this.transact(() => {
      for (const [k, v] of Object.entries(patch)) this.meta.set(k, v)
    })
  }
}

const num = (v: unknown) => typeof v === 'number' && Number.isFinite(v)

/**
 * Elements come from other people's machines: anything malformed is ignored instead of
 * breaking rendering. Image ids are checked so a guest can't point others' browsers at
 * an outside URL (which would reveal their IP).
 */
export function isValid(el: El): boolean {
  if (!el || typeof el !== 'object' || typeof el.id !== 'string') return false
  if (![el.x, el.y, el.w, el.h, el.rotation, el.z, el.opacity].every(num)) return false
  switch (el.type) {
    case 'ink':
    case 'highlighter':
      return Array.isArray(el.points) && el.points.length >= 3 && el.points.length < 200_000 && num(el.size) && typeof el.color === 'string'
    case 'line':
      return Array.isArray(el.points) && el.points.length === 4 && el.points.every(num)
    case 'shape':
      return typeof el.shape === 'string' && typeof el.fill === 'string' && typeof el.stroke === 'string'
    case 'text':
    case 'sticky':
      return typeof el.text === 'string' && el.text.length < 100_000
    case 'image':
      return typeof el.fileId === 'string' && /^[A-Za-z0-9_-]{6,64}$/.test(el.fileId)
    case 'stamp':
      return typeof el.emoji === 'string' && el.emoji.length <= 16
    default:
      return false
  }
}

/** Safe CSS colour from shared data: only hex and the keyword transparent. */
export const safeColor = (c: string, fallback = '#1E1E1E') => (/^#[0-9a-f]{3,8}$/i.test(c) || c === 'transparent' ? c : fallback)
