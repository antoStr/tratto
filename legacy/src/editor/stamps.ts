// Reactions are drawn from bundled Microsoft Fluent Emoji (MIT, see assets/emoji/LICENSE)
// instead of the system emoji font, so they look the same on every computer and in exports.
import bulb from '../assets/emoji/bulb.svg'
import check from '../assets/emoji/check.svg'
import clap from '../assets/emoji/clap.svg'
import cross from '../assets/emoji/cross.svg'
import exclamation from '../assets/emoji/exclamation.svg'
import eyes from '../assets/emoji/eyes.svg'
import fire from '../assets/emoji/fire.svg'
import heart from '../assets/emoji/heart.svg'
import hundred from '../assets/emoji/hundred.svg'
import laugh from '../assets/emoji/laugh.svg'
import party from '../assets/emoji/party.svg'
import pin from '../assets/emoji/pin.svg'
import question from '../assets/emoji/question.svg'
import rocket from '../assets/emoji/rocket.svg'
import star from '../assets/emoji/star.svg'
import thumbsUp from '../assets/emoji/thumbs-up.svg'

export interface Stamp {
  emoji: string
  name: string
  url: string
}

export const STAMP_SET: Stamp[] = [
  { emoji: '👍', name: 'Mi piace', url: thumbsUp },
  { emoji: '❤️', name: 'Cuore', url: heart },
  { emoji: '⭐', name: 'Stella', url: star },
  { emoji: '✅', name: 'Fatto', url: check },
  { emoji: '❌', name: 'No', url: cross },
  { emoji: '❓', name: 'Domanda', url: question },
  { emoji: '❗', name: 'Importante', url: exclamation },
  { emoji: '💡', name: 'Idea', url: bulb },
  { emoji: '🎉', name: 'Festa', url: party },
  { emoji: '🔥', name: 'Fuoco', url: fire },
  { emoji: '👀', name: 'Da guardare', url: eyes },
  { emoji: '😂', name: 'Risata', url: laugh },
  { emoji: '👏', name: 'Applauso', url: clap },
  { emoji: '💯', name: 'Perfetto', url: hundred },
  { emoji: '🚀', name: 'Via!', url: rocket },
  { emoji: '📌', name: 'Puntina', url: pin },
]

const byEmoji = new Map(STAMP_SET.map((s) => [s.emoji, s]))
export const stampInfo = (emoji: string) => byEmoji.get(emoji)

/** Rendered once at this size so stamps stay sharp when zoomed in. */
const RASTER = 512
const bitmaps = new Map<string, ImageBitmap | 'loading' | 'failed'>()
const listeners = new Set<() => void>()

/** Called when a stamp picture finishes loading, to repaint the board. */
export function onStampLoaded(fn: () => void) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

async function rasterize(url: string) {
  // Give the SVG a large intrinsic size so the browser rasterises it sharply.
  const svg = (await (await fetch(url)).text()).replace(/width="\d+" height="\d+"/, `width="${RASTER}" height="${RASTER}"`)
  const blobUrl = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }))
  try {
    const img = new Image()
    img.src = blobUrl
    await img.decode()
    return await createImageBitmap(img, { resizeWidth: RASTER, resizeHeight: RASTER })
  } finally {
    URL.revokeObjectURL(blobUrl)
  }
}

/** The stamp picture if ready; starts loading it otherwise. Null for unknown emoji (drawn as text). */
export function stampBitmap(emoji: string): ImageBitmap | null {
  const info = byEmoji.get(emoji)
  if (!info) return null
  const cached = bitmaps.get(emoji)
  if (cached instanceof ImageBitmap) return cached
  if (!cached) {
    bitmaps.set(emoji, 'loading')
    rasterize(info.url)
      .then((bmp) => bitmaps.set(emoji, bmp))
      .catch(() => bitmaps.set(emoji, 'failed'))
      .finally(() => listeners.forEach((l) => l()))
  }
  return null
}

/** Loads every stamp up front (exports need them all ready). */
export function preloadStamps() {
  return Promise.all(
    STAMP_SET.map(
      (s) =>
        new Promise<void>((done) => {
          if (stampBitmap(s.emoji)) return done()
          const off = onStampLoaded(() => {
            if (bitmaps.get(s.emoji) !== 'loading') {
              off()
              done()
            }
          })
        }),
    ),
  )
}

/** SVG markup data URL for vector exports. */
export async function stampDataUrl(emoji: string): Promise<string | null> {
  const info = byEmoji.get(emoji)
  if (!info) return null
  const svg = await (await fetch(info.url)).text()
  return `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`
}
