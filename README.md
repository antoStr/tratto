# Tratto

Lavagna infinita per Windows, Mac e Linux con l'interfaccia di Figma e gli strumenti di Microsoft Whiteboard: penna sensibile alla pressione, evidenziatore, gomma, righello, lazo, forme, testo, note adesive, immagini, reazioni, puntatore laser e modelli. Le lavagne restano sul PC; quando serve, un link fa entrare altre persone dal browser.

## Per chi la usa

**Installazione.** Scarica il file giusto da https://github.com/antoStr/tratto/releases/latest:
- **Windows**: `Tratto-Setup-X.Y.Z.exe`. L'installer non è firmato digitalmente, quindi SmartScreen può avvisare: *Ulteriori informazioni → Esegui comunque*. Le lavagne si salvano da sole in `%APPDATA%\Tratto`. Disinstallando, Tratto toglie tutto quello che ha lasciato sul PC e chiede se cancellare anche le lavagne (di base le tiene).
- **Mac**: `Tratto-X.Y.Z-mac-arm64.dmg` per i Mac con chip Apple (M1 e successivi), `…-mac-x64.dmg` per quelli Intel. Trascina Tratto in Applicazioni. Non avendo un certificato Apple a pagamento, al primo avvio macOS lo blocca: *Impostazioni di Sistema ▸ Privacy e sicurezza ▸ Apri comunque*. Su Mac gli aggiornamenti si scaricano a mano da questa pagina.
- **Linux**: `Tratto-X.Y.Z-linux-amd64.deb` per Ubuntu e Debian (`sudo apt install ./Tratto-….deb`, si disinstalla con `sudo apt remove tratto`), oppure `Tratto-X.Y.Z-linux-x86_64.AppImage` per le altre distribuzioni (rendilo eseguibile e avvialo).

Per **entrare** in una lavagna condivisa non serve installare niente, su nessun sistema: basta aprire il link nel browser.

**Condividere una lavagna.**
1. Apri la lavagna e premi **Condividi**.
2. Scegli se chi ha il link *può modificare* o *può solo guardare*, poi **Crea link di invito**. In pochi secondi arriva un link come `https://parole-a-caso.trycloudflare.com/#codice`.
3. Mandalo a chi vuoi (fino a 30 persone insieme). Chi lo apre scrive il proprio nome e **chiede di entrare**: decidi tu con *Fai entrare* o *Rifiuta*. Puoi saltare questo passaggio con *Fai entrare senza chiedermelo*.
4. **Rimuovi** toglie l'accesso a una persona. **Nuovo link** invalida quello vecchio. **Termina condivisione** (o chiudere Tratto) spegne tutto: il link smette di funzionare.

Il collegamento passa da un tunnel cifrato di Cloudflare. Il PC si collega solo in uscita: nessuna porta aperta sul router e il tuo indirizzo IP non è visibile agli ospiti. Gli ospiti non vedono nemmeno gli indirizzi l'uno dell'altro.

**Penna e touch.** Con la penna: il tasto laterale trascinato seleziona col lazo, toccato apre il menu; la parte superiore cancella. Tenendo premuta la penna o il dito su un elemento si apre il menu. Con due dita sposti e ingrandisci la lavagna. Il palmo appoggiato mentre scrivi viene ignorato.

**Livelli e cartelle.** Nel pannello *Livelli* i tratti scritti di seguito nello stesso punto stanno insieme in una riga *Scrittura*. Seleziona e premi `Ctrl+G` (o tasto destro ▸ *Metti in una nuova cartella*) per creare una cartella con nome, da aprire, bloccare e nascondere. Un clic su un livello o una cartella porta la vista lì. La minimappa in alto a destra e il pulsante *Torna ai contenuti* aiutano a non perdersi.

**Solo la lavagna.** `Ctrl+\` (o il pulsante accanto al nome) nasconde entrambi i pannelli: restano la barra degli strumenti e una piccola pillola per riaprirli.

**Impostazioni** (`Ctrl+,`, anche dalla schermata delle lavagne): tema, dimensione dell'interfaccia e del testo, contrasto elevato, maniglie più grandi, colore principale, posizione della barra, pannelli, minimappa, sfondo delle nuove lavagne, levigatura e pressione della penna, aggiornamenti.

**Esportare.** *Menu ▸ Esporta* (`Ctrl+Maiusc+E`) o tasto destro ▸ *Esporta selezione*: PNG, JPG, SVG, PDF (anche su A4 da stampare) o file Tratto. *Copia* mette l'immagine negli appunti, da incollare in chat o documenti.

**Righello.** Premi `U` o il pulsante col righello. Trascinalo con dito o mouse e ruotalo con la rotellina, con due dita o con la manopola. Una linea iniziata vicino al bordo viene tracciata dritta lungo il righello.

Tutte le scorciatoie sono nel menu, alla voce *Scorciatoie da tastiera* (`?`).

## Aggiornamenti automatici

Su Windows e Linux Tratto controlla da solo se esiste una nuova versione (dopo l'avvio e poi ogni 4 ore), la scarica in background e, quando è pronta, compare in alto il pulsante **Riavvia per aggiornare**. Se preferisci, l'aggiornamento si installa comunque alla prossima chiusura. Le lavagne non si toccano. Da *Impostazioni ▸ Aggiornamenti* puoi vedere la versione e cercare aggiornamenti a mano.

Chi sviluppa: `npm run release` (o `npm run release:patch`) alza la versione, crea il tag `vX.Y.Z` e lo carica su GitHub; l'Action `build` costruisce in parallelo su Windows, Mac e Linux e pubblica una sola release con tutti gli installer, i `.blockmap` e i `latest*.yml` letti dagli aggiornamenti. I Tratto già installati si aggiornano da soli. La prima installazione di questa versione va fatta a mano da https://github.com/antoStr/tratto/releases/latest (le versioni precedenti non sanno aggiornarsi).

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
- Il modo più semplice è la GitHub Action `.github/workflows/build.yml` (avviabile anche a mano da *Actions*), che produce gli installer per tutti e tre i sistemi.
- Linux in locale: `npm run build && node scripts/fetch-cloudflared.ts linux && npx electron-builder --linux AppImage --publish never`. Il `.deb` richiede `libcrypt.so.1` (su Fedora: `libxcrypt-compat`).
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
- Un massimo di 30 ospiti insieme e 30 richieste in attesa; con più di 10 persone i cursori si aggiornano meno spesso, per non saturare la connessione di chi ospita.
- Messaggi grandi al massimo 4 MB.
- Upload solo di immagini PNG, JPEG, GIF o WebP, riconosciute dai primi byte, fino a 300 MB per sessione.
- Ogni elemento ricevuto viene validato prima di disegnarlo.
- Le immagini si caricano solo dal server locale, così nessuno può far contattare server esterni agli altri partecipanti.
- CSP restrittiva e HSTS.

### Limiti noti
- Il link `trycloudflare.com` cambia a ogni condivisione. Per un indirizzo fisso, ad esempio `lavagna.tuodominio.it/#codice`, serve un tunnel Cloudflare con nome e un dominio tuo.
- Tutti gli ospiti condividono un solo link per volta, legato a una sola lavagna.
- Mancano ancora tabelle, inserimento di PDF e commenti.
