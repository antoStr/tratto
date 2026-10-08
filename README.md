# Tratto

Lavagna infinita per Windows 11 con l'interfaccia di Figma e gli strumenti di Microsoft Whiteboard: penna sensibile alla pressione, evidenziatore, gomma, righello, lazo, forme, testo, note adesive, immagini, reazioni, puntatore laser e modelli. Le lavagne restano sul PC; quando serve, un link fa entrare altre persone dal browser.

## Per chi la usa

**Installazione.** Avvia `Tratto-Setup-1.0.0.exe`. L'installer non è firmato digitalmente, quindi Windows SmartScreen può avvisare: *Ulteriori informazioni → Esegui comunque*. Le lavagne si salvano da sole in `%APPDATA%\Tratto` e restano anche se disinstalli.

**Condividere una lavagna.**
1. Apri la lavagna e premi **Condividi**.
2. Scegli se chi ha il link *può modificare* o *può solo guardare*, poi **Crea link di invito**. In pochi secondi arriva un link come `https://parole-a-caso.trycloudflare.com/#codice`.
3. Mandalo a chi vuoi. Chi lo apre scrive il proprio nome e **chiede di entrare**: decidi tu con *Fai entrare* o *Rifiuta*. Puoi saltare questo passaggio con *Fai entrare senza chiedermelo*.
4. **Rimuovi** toglie l'accesso a una persona. **Nuovo link** invalida quello vecchio. **Termina condivisione** (o chiudere Tratto) spegne tutto: il link smette di funzionare.

Il collegamento passa da un tunnel cifrato di Cloudflare. Il PC si collega solo in uscita: nessuna porta aperta sul router e il tuo indirizzo IP non è visibile agli ospiti. Gli ospiti non vedono nemmeno gli indirizzi l'uno dell'altro.

**Penna e touch.** Con la penna: il tasto laterale attiva il lazo e la parte superiore cancella. Con due dita sposti e ingrandisci la lavagna. Il palmo appoggiato mentre scrivi viene ignorato. *Menu ▸ Preferenze* permette di disegnare con le dita o di fare zoom con la rotellina.

**Righello.** Premi `U` o il pulsante col righello. Trascinalo con dito o mouse e ruotalo con la rotellina, con due dita o con la manopola. Una linea iniziata vicino al bordo viene tracciata dritta lungo il righello.

Tutte le scorciatoie sono nel menu, alla voce *Scorciatoie da tastiera* (`?`).

## Aggiornamenti automatici

Tratto controlla da solo se esiste una nuova versione (dopo l'avvio e poi ogni 4 ore), la scarica in background e, quando è pronta, compare in alto il pulsante **Riavvia per aggiornare**. Se preferisci, l'aggiornamento si installa comunque alla prossima chiusura. Le lavagne non si toccano. Da *Impostazioni ▸ Aggiornamenti* puoi vedere la versione e cercare aggiornamenti a mano.

Chi sviluppa: `npm run release` (o `npm run release:patch`) alza la versione, crea il tag `vX.Y.Z` e lo carica su GitHub; l'Action `build-windows` costruisce l'installer e pubblica la release con `.exe`, `.blockmap` e `latest.yml`. I Tratto già installati si aggiornano da soli. La prima installazione di questa versione va fatta a mano da https://github.com/antoStr/tratto/releases/latest (le versioni precedenti non sanno aggiornarsi).

## Per chi la sviluppa

Requisiti: Node 22.18 o superiore (TypeScript e SQLite integrati, nessuna compilazione nativa).

```sh
npm install
npm run fetch:tunnel        # cloudflared per Linux, verificato con SHA256
npm run dev                 # server + UI con Vite; apri il link stampato nel terminale
npm run app:dev             # la stessa cosa dentro Electron
npm test                    # test del server (sicurezza, condivisione, Yjs) e del motore della lavagna
npm run typecheck
```

**Installer Windows.**
- Il modo più semplice è la GitHub Action `.github/workflows/build-windows.yml`, che produce l'`.exe` su `windows-latest`.
- Da Linux serve wine. Senza wine installato, usa il container di electron-builder (servono circa 5 GB liberi):

```sh
npm run build && node scripts/fetch-cloudflared.ts win
podman run --rm -v "$PWD":/project:Z -w /project docker.io/electronuserland/builder:wine \
  npx electron-builder --win --x64 --publish never
```

### Come è fatta

| Parte | File | Note |
|---|---|---|
| App desktop | `electron/main.ts` | Finestra sandboxed; naviga solo verso il server locale; spegne la condivisione e salva tutto alla chiusura. |
| Server locale | `server/index.ts` | Due server su `127.0.0.1`. Quello **privato** è usato solo dalla finestra dell'app: cookie HttpOnly e controllo di `Host`/`Origin` contro il DNS rebinding. Quello **pubblico** è l'unico raggiunto dal tunnel ed espone solo le rotte per gli ospiti. |
| Collaborazione | Hocuspocus + Yjs | Modifiche senza conflitti e annulla per singolo utente. I permessi di sola lettura sono applicati lato server. |
| Condivisione | `server/tunnel.ts` | Cloudflare quick tunnel. Il codice di sessione (128 bit) sta dopo `#` e quindi non finisce nei log. Gli ospiti passano dalla sala d'attesa e ricevono un biglietto revocabile. |
| Dati | `server/db.ts` | SQLite in `%APPDATA%\Tratto\tratto.db`: lavagne come stato Yjs, immagini, miniature. |
| Motore della lavagna | `src/editor/controller.ts`, `render.ts`, `geometry.ts`, `ink.ts` | Canvas 2D con culling, tratti `perfect-freehand`, riconoscimento delle forme, righello e aggancio magnetico. |
| Interfaccia | `src/editor/*.tsx`, `src/home`, `src/styles.css` | Token di Figma UI3 (Inter 11px, controlli da 24px, pannelli agganciati, menu scuri), tema chiaro e scuro, accessibilità con Radix. |

**Difese per gli ospiti.**
- Un limite di tentativi falliti per IP.
- Un massimo di 20 ospiti e 30 richieste in attesa.
- Messaggi grandi al massimo 4 MB.
- Upload solo di immagini PNG, JPEG, GIF o WebP, riconosciute dai primi byte, fino a 300 MB per sessione.
- Ogni elemento ricevuto viene validato prima di disegnarlo.
- Le immagini si caricano solo dal server locale, così nessuno può far contattare server esterni agli altri partecipanti.
- CSP restrittiva e HSTS.

### Limiti noti
- Il link `trycloudflare.com` cambia a ogni condivisione. Per un indirizzo fisso, ad esempio `lavagna.tuodominio.it/#codice`, serve un tunnel Cloudflare con nome e un dominio tuo.
- Tutti gli ospiti condividono un solo link per volta, legato a una sola lavagna.
- Mancano ancora gruppi, tabelle, inserimento di PDF e commenti.
