# Tratto

Lavagna infinita per Windows, Mac e Linux con l'interfaccia di Figma e FigJam e gli strumenti di Microsoft Whiteboard: penna sensibile alla pressione, evidenziatore, nastro adesivo, gomma, righello, lazo, 21 forme, connettori dritti, a gomito o curvi con etichetta, testo con elenchi, note adesive, tabelle, blocchi di codice, widget (sondaggio, lista di cose da fare, contatore), sezioni, immagini, reazioni, commenti, timer, votazioni, puntatore laser e modelli. Le lavagne restano sul PC; quando serve, un link fa entrare altre persone dal browser.

Dalla versione 2 Tratto è un'app nativa scritta in Rust con [egui]: si apre in un attimo e usa circa un quinto della memoria della versione 1 (Electron). Le lavagne della versione 1 si aprono senza modifiche.

## Per chi la usa

**Installazione.** Scarica il file giusto da https://github.com/antoStr/tratto/releases/latest:
- **Windows**: il file che finisce con `-setup.exe`. L'installer non è firmato digitalmente, quindi SmartScreen può avvisare: *Ulteriori informazioni → Esegui comunque*. Le lavagne si salvano da sole in `%APPDATA%\Tratto`. Se c'era Tratto 1, l'installer lo toglie e tiene le lavagne. Disinstallando, Tratto chiede se cancellare anche le lavagne (di base le tiene).
- **Mac**: il `.dmg` con `aarch64` per i Mac con chip Apple (M1 e successivi), quello con `x64` per i Mac Intel. Trascina Tratto in Applicazioni. Non avendo un certificato Apple a pagamento, al primo avvio macOS lo blocca: *Impostazioni di Sistema ▸ Privacy e sicurezza ▸ Apri comunque*.
- **Linux**: il `.deb` per Ubuntu e Debian (`sudo apt install ./tratto_….deb`, si disinstalla con `sudo apt remove tratto`), oppure l'`.AppImage` per le altre distribuzioni (rendilo eseguibile e avvialo).

Per **entrare** in una lavagna condivisa non serve installare niente, su nessun sistema: basta aprire il link nel browser.

**Condividere una lavagna.**
1. Apri la lavagna e premi **Condividi**.
2. Scegli se chi ha il link *può modificare* o *può solo guardare*, poi **Crea link di invito**. In pochi secondi arriva un link come `https://parole-a-caso.trycloudflare.com/#codice`.
3. Mandalo a chi vuoi (fino a 30 persone insieme). Chi lo apre scrive il proprio nome e **chiede di entrare**: decidi tu con *Fai entrare* o *Rifiuta*. Puoi saltare questo passaggio con *Fai entrare senza chiedermelo*.
4. **Rimuovi** toglie l'accesso a una persona. **Nuovo link** invalida quello vecchio. **Termina condivisione** (o chiudere Tratto) spegne tutto: il link smette di funzionare.

Il collegamento passa da un tunnel cifrato di Cloudflare. Il PC si collega solo in uscita: nessuna porta aperta sul router e il tuo indirizzo IP non è visibile agli ospiti. Gli ospiti non vedono nemmeno gli indirizzi l'uno dell'altro.

**Penna e touch.** Con la penna: il tasto laterale trascinato seleziona col lazo, toccato apre il menu; la parte superiore cancella. Tenendo premuta la penna o il dito su un elemento si apre il menu. Con due dita sposti e ingrandisci la lavagna.

**Impostazioni** (`Ctrl+,`, anche dalla schermata delle lavagne): tema, dimensione dell'interfaccia e del testo, contrasto elevato, colore principale, posizione della barra, pannelli, minimappa, penna, aggiornamenti.

**Come in FigJam.** Selezionando qualcosa compare sopra una barra scura con i suoi colori, la forma, il carattere e le altre impostazioni. Il **+** della barra degli strumenti aggiunge blocchi di codice e widget. *Tabella*: doppio clic su una cella per scrivere, Tab per passare alla successiva (nell'ultima aggiunge una riga). *Connettori*: doppio clic su una linea per scriverci un'etichetta. *Widget*: si cliccano direttamente sulla lavagna (con lo strumento di selezione) e si scrivono nel pannello a destra.

**Esportare.** *Menu ▸ Esporta*: PNG, JPG, SVG, PDF o file Tratto.

Tutte le scorciatoie sono nel menu, alla voce *Scorciatoie da tastiera* (`?`).

**Aggiornamenti.** All'avvio Tratto controlla se c'è una versione nuova: se c'è, nella schermata delle lavagne compare *Scarica Tratto X.Y.Z*, che apre la pagina da cui scaricarla. Le lavagne non si toccano. Chi ha ancora Tratto 1 su Windows o in AppImage riceve la versione 2 da solo, alla chiusura dell'app.

## Per chi la sviluppa

Requisiti: Rust stabile, più `rustup target add wasm32-unknown-unknown` e `cargo install wasm-bindgen-cli --version 0.2.129` per la pagina degli ospiti.

```sh
sh scripts/web.sh           # la pagina degli ospiti (la stessa app in WebAssembly) in web/pkg
cargo run --release         # l'app; per condividere serve bin/cloudflared
cargo test
```

**Installer.** La GitHub Action `.github/workflows/build.yml` costruisce su Windows (NSIS), Mac (dmg arm64 e x64) e Linux (AppImage e deb) con [cargo-packager]. Su un tag `vX.Y.Z` pubblica una sola release con tutti i file, più `latest.yml` e `latest-linux.yml` che fanno aggiornare da soli i Tratto 1.x. Per una nuova versione: alza `version` in `Cargo.toml`, crea il tag annotato `vX.Y.Z` e caricalo.

### Come è fatta

| Parte | File | Note |
|---|---|---|
| App | `src/main.rs`, `src/app/` | egui/eframe con wgpu; schermata delle lavagne, lavagna, pannelli, finestre di dialogo. Su Linux usa OpenGL sulla scheda integrata: Vulkan sveglierebbe la scheda dedicata (2 secondi all'avvio, più batteria). |
| Disegno | `src/paint.rs`, `src/prims.rs`, `src/text.rs` | Ogni elemento diventa una mesh (lyon) tenuta in cache; il testo usa rustybuzz con i font in `assets/fonts`. Con la lavagna rimpicciolita i tratti perdono i punti superflui e il testo troppo piccolo diventa una barretta. |
| Dati | `src/store.rs`, `src/doc.rs` | SQLite in `%APPDATA%\Tratto\tratto.db`, lavagne come stato Yjs (yrs): lo stesso formato della versione 1. |
| Condivisione | `src/share.rs`, `src/guest.rs` | Server locale (axum) raggiunto solo dal Cloudflare quick tunnel. Il codice di sessione sta dopo `#` e non finisce nei log; gli ospiti passano dalla sala d'attesa e ricevono un biglietto revocabile. Gli ospiti usano la stessa app compilata in WebAssembly. |

### Limiti noti
- Il link `trycloudflare.com` cambia a ogni condivisione. Per un indirizzo fisso serve un tunnel Cloudflare con nome e un dominio tuo.
- Tutti gli ospiti condividono un solo link per volta, legato a una sola lavagna.
- Dei servizi di FigJam mancano quelli che richiedono un cloud: l'intelligenza artificiale, i plugin e i widget della community, la musica del timer.

[egui]: https://github.com/emilk/egui
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager
