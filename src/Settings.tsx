import { useEffect, useState, type ReactNode } from 'react'
import { Tabs } from 'radix-ui'
import { bridge } from './desktop.ts'
import { BackgroundFields } from './editor/Inspector.tsx'
import { DEFAULT_ACCENT, useEditor, type Prefs } from './editor/store.ts'
import { Dialog, Segmented, Switch, Tip } from './ui.tsx'
import { UpdatesSettings } from './updates.tsx'

/** Opens the settings dialog from anywhere (menus, home screen). Also Ctrl+, */
export const openSettings = () => window.dispatchEvent(new Event('tratto:settings'))

const ACCENTS = [
  { value: DEFAULT_ACCENT, name: 'Blu' },
  { value: '#7B61FF', name: 'Viola' },
  { value: '#E84393', name: 'Rosa' },
  { value: '#E03131', name: 'Rosso' },
  { value: '#F76707', name: 'Arancione' },
  { value: '#14AE5C', name: 'Verde' },
  { value: '#0C8599', name: 'Petrolio' },
]
const SCALES = ['0.9', '1', '1.15', '1.3', '1.5']

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="set-row">
      <div>
        <div className="set-label">{label}</div>
        {hint && <p className="hint">{hint}</p>}
      </div>
      <div className="set-control">{children}</div>
    </div>
  )
}

export function SettingsDialog({ host }: { host: boolean }) {
  const [open, setOpen] = useState(false)
  const prefs = useEditor((s) => s.prefs)
  const setPrefs = useEditor((s) => s.setPrefs)

  useEffect(() => {
    const show = () => setOpen(true)
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ',') {
        e.preventDefault()
        setOpen(true)
      }
    }
    window.addEventListener('tratto:settings', show)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('tratto:settings', show)
      window.removeEventListener('keydown', onKey)
    }
  }, [])

  const toggle = (k: keyof Prefs, label: string, hint?: string) => (
    <div className="set-switch">
      <Switch id={`set-${k}`} label={label} checked={!!prefs[k]} onChange={(v) => setPrefs({ [k]: v } as Partial<Prefs>)} />
      {hint && <p className="hint">{hint}</p>}
    </div>
  )
  const updates = host && !!bridge

  return (
    <Dialog open={open} onOpenChange={setOpen} title="Impostazioni" wide>
      <Tabs.Root defaultValue="a11y" className="settings">
        <Tabs.List className="tabs set-tabs" aria-label="Sezioni delle impostazioni">
          <Tabs.Trigger value="a11y" className="tab-text">
            Accessibilità
          </Tabs.Trigger>
          <Tabs.Trigger value="look" className="tab-text">
            Personalizzazione
          </Tabs.Trigger>
          <Tabs.Trigger value="pen" className="tab-text">
            Penna
          </Tabs.Trigger>
          {updates && (
            <Tabs.Trigger value="updates" className="tab-text">
              Aggiornamenti
            </Tabs.Trigger>
          )}
        </Tabs.List>

        <Tabs.Content value="a11y" className="set-panel">
          <Row label="Tema">
            <Segmented<Prefs['theme']>
              label="Tema"
              value={prefs.theme}
              onChange={(theme) => setPrefs({ theme })}
              options={[
                { value: 'system', label: 'Sistema' },
                { value: 'light', label: 'Chiaro' },
                { value: 'dark', label: 'Scuro' },
              ]}
            />
          </Row>
          {bridge && (
            <Row label="Dimensione dell'interfaccia" hint="Ingrandisce menu, pannelli e testi.">
              <Segmented<string>
                label="Dimensione dell'interfaccia"
                value={String(prefs.uiScale)}
                onChange={(v) => setPrefs({ uiScale: Number(v) })}
                options={SCALES.map((s) => ({ value: s, label: `${Math.round(Number(s) * 100)}%` }))}
              />
            </Row>
          )}
          <Row label="Dimensione del testo" hint="Solo i testi dell'interfaccia, non quelli sulla lavagna.">
            <Segmented<string>
              label="Dimensione del testo"
              value={String(prefs.textScale)}
              onChange={(v) => setPrefs({ textScale: Number(v) })}
              options={[
                { value: '1', label: 'Normale' },
                { value: '1.18', label: 'Grande' },
                { value: '1.36', label: 'Molto grande' },
              ]}
            />
          </Row>
          {toggle('highContrast', 'Contrasto elevato', 'Testi secondari più scuri e bordi ben visibili.')}
          {toggle('bigHandles', 'Maniglie di selezione più grandi', 'Più facili da prendere con la penna e con le dita.')}
          <p className="hint">Se in Windows hai ridotto le animazioni, Tratto le riduce allo stesso modo.</p>
        </Tabs.Content>

        <Tabs.Content value="look" className="set-panel">
          <Row label="Colore principale" hint="Per selezione, pulsanti ed evidenziazioni.">
            <div className="accent-swatches" role="radiogroup" aria-label="Colore principale">
              {ACCENTS.map((a) => (
                <Tip key={a.value} label={a.name}>
                  <button type="button" role="radio" aria-checked={prefs.accent.toUpperCase() === a.value} aria-label={a.name} className="bg-swatch accent-swatch" style={{ background: a.value }} onClick={() => setPrefs({ accent: a.value })} />
                </Tip>
              ))}
              <label className="bg-swatch accent-swatch custom" title="Colore personalizzato">
                <span className="sr-only">Colore personalizzato</span>
                <input type="color" value={prefs.accent} onChange={(e) => setPrefs({ accent: e.target.value.toUpperCase() })} />
              </label>
            </div>
          </Row>
          <Row label="Barra degli strumenti">
            <Segmented<Prefs['toolbarPos']>
              label="Posizione della barra degli strumenti"
              value={prefs.toolbarPos}
              onChange={(toolbarPos) => setPrefs({ toolbarPos })}
              options={[
                { value: 'bottom', label: 'In basso' },
                { value: 'top', label: 'In alto' },
              ]}
            />
          </Row>
          <h3 className="set-heading">Pannelli</h3>
          {toggle('leftPanel', 'Livelli e modelli, a sinistra')}
          {toggle('rightPanel', 'Design e condivisione, a destra')}
          {toggle('minimap', 'Minimappa', 'Mostra tutta la lavagna in piccolo: clicca per spostarti.')}
          {host && (
            <>
              <h3 className="set-heading">Sfondo delle nuove lavagne</h3>
              <BackgroundFields value={prefs.newBoard} onChange={(patch) => setPrefs({ newBoard: { ...prefs.newBoard, ...patch } })} />
            </>
          )}
        </Tabs.Content>

        <Tabs.Content value="pen" className="set-panel">
          <Row label="Levigatura del tratto" hint="Più alta toglie il tremolio, più bassa segue ogni movimento.">
            <Segmented<Prefs['inkSmoothing']>
              label="Levigatura del tratto"
              value={prefs.inkSmoothing}
              onChange={(inkSmoothing) => setPrefs({ inkSmoothing })}
              options={[
                { value: 'low', label: 'Bassa' },
                { value: 'medium', label: 'Media' },
                { value: 'high', label: 'Alta' },
              ]}
            />
          </Row>
          {toggle('pressure', 'Spessore secondo la pressione', 'Premendo di più con la penna il tratto diventa più spesso.')}
          {toggle('inkToShape', 'Da tratto a forma', 'Cerchi, rettangoli, triangoli e linee disegnati a mano diventano forme pulite.')}
          {toggle('fingerDraw', 'Disegna con le dita', 'Se è spento, le dita spostano la lavagna e solo la penna disegna.')}
          <div className="set-switch">
            <Switch id="set-wheel" label="La rotellina del mouse fa zoom" checked={prefs.wheel === 'zoom'} onChange={(v) => setPrefs({ wheel: v ? 'zoom' : 'pan' })} />
          </div>
          <p className="hint">Tasto laterale della penna: trascina per selezionare col lazo, tocca per aprire il menu. Tieni premuta la penna o il dito su un elemento per aprire il menu.</p>
        </Tabs.Content>

        {updates && (
          <Tabs.Content value="updates" className="set-panel">
            <UpdatesSettings />
          </Tabs.Content>
        )}
      </Tabs.Root>
    </Dialog>
  )
}
