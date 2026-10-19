# Tratto

Lavagna infinita per Windows, Mac e Linux con un'interfaccia moderna, curata e completa: penna sensibile alla pressione con otto pennelli (penna, matita, pennarello, calligrafica, pennello, aerografo, gessetto, sfumino), stabilizzatore, forme perfette e simmetria avanzata, evidenziatore, nastro adesivo, gomma, righello, lazo, 21 forme, connettori dritti, a gomito o curvi con etichetta, testo con elenchi, note adesive, tabelle, blocchi di codice, widget (sondaggio, lista di cose da fare, contatore), sezioni, immagini, reazioni (emoji 3D), commenti, chat vicino al cursore, timer, votazioni, puntatore laser e modelli. Le lavagne restano sul PC; quando serve, un link fa entrare altre persone dal browser.

Dalla versione 2 Tratto è un'app nativa scritta in Rust con [egui]: si apre in un attimo e usa circa un quinto della memoria della versione 1 (Electron). Le lavagne della versione 1 si aprono senza modifiche.

## Per chi la usa

**Installazione.** Scarica il file giusto da https://github.com/antoStr/tratto/releases/latest:
- **Windows**: il file che finisce con `-setup.exe`. L'installer non è firmato digitalmente, quindi SmartScreen può avvisare: *Ulteriori informazioni → Esegui comunque*. Le lavagne si salvano da sole in `%APPDATA%\Tratto`. Se c'era Tratto 1, l'installer lo toglie e tiene le lavagne. Disinstallando, Tratto chiede se cancellare anche le lavagne (di base le tiene).
- **Mac**: il `.dmg` con `aarch64` per i Mac con chip Apple (M1 e successivi), quello con `x64` per i Mac Intel. Trascina Tratto in Applicazioni. Non avendo un certificato Apple a pagamento, al primo avvio macOS lo blocca: *Impostazioni di Sistema ▸ Privacy e sicurezza ▸ Apri comunque*.
- **Linux**: il `.rpm` per Fedora e openSUSE (`sudo dnf install ./tratto-….rpm`, si disinstalla con `sudo dnf remove tratto`), il `.deb` per Ubuntu e Debian (`sudo apt install ./tratto_….deb`, si disinstalla con `sudo apt remove tratto`), oppure l'`.AppImage` per tutte le altre: rendila eseguibile, avviala e premi *Aggiungi al menu delle app* (nessuna password: si copia in `~/.local/share/tratto`; si toglie da *Impostazioni ▸ Aggiornamenti*).

Per **entrare** in una lavagna condivisa non serve installare niente, su nessun sistema: basta aprire il link nel browser.

**Condividere una lavagna.**
1. Apri la lavagna e premi **Condividi**.
2. Scegli se chi ha il link *può modificare* o *può solo guardare*, poi **Crea link di invito**. In pochi secondi arriva un link come `https://parole-a-caso.trycloudflare.com/#codice`.
3. Mandalo a chi vuoi (fino a 30 persone insieme); dal telefono basta inquadrare il QR code sotto il link. Chi lo apre scrive il proprio nome e **chiede di entrare**: decidi tu con *Fai entrare* o *Rifiuta*. Puoi saltare questo passaggio con *Fai entrare senza chiedermelo*.
4. **Rimuovi** toglie l'accesso a una persona. **Nuovo link** invalida quello vecchio. **Termina condivisione** (o chiudere Tratto) spegne tutto: il link smette di funzionare.

Il collegamento passa da un tunnel cifrato di Cloudflare. Il PC si collega solo in uscita: nessuna porta aperta sul router e il tuo indirizzo IP non è visibile agli ospiti. Gli ospiti non vedono nemmeno gli indirizzi l'uno dell'altro.

**Pennelli.** Ognuna delle quattro penne della barra ha il suo pennello. Tutto quello che riguarda la penna in mano sta nel pannello di destra (*Personalizza* nella barra, o tocca di nuovo la penna attiva): il pennello, il colore (anche preso dalla lavagna), spessore, opacità e stabilizzatore, la pressione e gli aiuti. I pennelli: *Penna* (sottile se tocchi appena, piena se premi), *Matita* (granulosa come la grafite sulla carta), *Pennarello* (stessa larghezza; gli incroci con altri tratti vengono più scuri), *Calligrafica* (un pennino piatto a 45°: largo scendendo a destra, un filo salendo), *Pennello* (punte affusolate; col mouse si assottiglia dove vai veloce), *Aerografo* (morbido, si accumula: per le sfumature), *Gessetto* (grana grossa), *Sfumino* (raccoglie i colori che ha sotto e li trascina, mescolandoli). Il pennello dei tratti già disegnati si cambia dalla barra scura che compare selezionandoli.

**Disegno avanzato.** *Stabilizzatore*: linee più pulite, senza il tremolio della mano. *Forma perfetta*: tieni ferma la penna a fine tratto e il cerchio, il rettangolo o la linea diventano perfetti, con il pennello che stavi usando. *Simmetria*: verticale, orizzontale, in quattro o in otto, con le guide sulla lavagna. Con la penna in mano: `Alt` + trascina per regolare spessore (orizzontale) e opacità/forza (verticale), `Alt` + clic per campionare un colore, `[` e `]` per lo spessore a passi.

**Pressione.** Con una penna Windows Ink (Surface, i due-in-uno, le tavolette con Windows Ink attivo) e anche con le tavolette Wacom, Huion o XP-Pen con Windows Ink spento nel loro pannello: in quel caso Tratto legge la pressione dal driver della tavoletta (Wintab). Nella pagina degli ospiti anche Apple Pencil e S Pen. Il tratto parte fine, si allarga con la pressione e si assottiglia sollevando la penna; un picco del driver non lascia macchie. La *Sensibilità* (leggera, normale, decisa) è nel pannello della penna e in *Impostazioni ▸ Penna*, dove un riquadro permette di provare la penna e mostra la pressione letta e da dove arriva.

**Penna e touch.** Con la penna: il tasto laterale trascinato seleziona col lazo, toccato apre il menu; la parte superiore cancella. Tenendo premuta la penna o il dito su un elemento si apre il menu. Con due dita sposti e ingrandisci la lavagna. Col mouse il tratto segue il puntatore senza levigatura, anche in corsivo veloce (la levigatura *Alta*, in Impostazioni ▸ Penna, la rimette); su Windows Tratto recupera anche i movimenti che il sistema accorpa mentre disegna, così nessun occhiello va perso, e non spezza il tratto quando il tasto del mouse "rimbalza" (un rilascio di pochi millesimi di secondo, frequente sui mouse da gaming). Con *Da tratto a forma* (nel pannellino della penna) linee, frecce, rettangoli anche inclinati, ellissi, rombi, triangoli, poligoni e stelle disegnati a mano diventano forme pulite. Sul righello si scrive appoggiandosi al bordo più vicino; con la penna lo si sposta dalla targhetta dei gradi.

**Impostazioni** (l'ingranaggio accanto alla casetta, o `Ctrl+,`): tema, dimensione dell'interfaccia e del testo, contrasto elevato, *riduci il movimento* (solo dissolvenze, niente scorrimenti), colore principale (anche personalizzato), posizione della barra, pannelli, minimappa, penna, scorciatoie, aggiornamenti. In *Profilo* il nome e il colore con cui ti vedono gli altri (cursore, selezione, commenti) e il colore del puntatore laser; si aprono anche cliccando il tuo avatar. In *Barra degli strumenti* scegli quali pulsanti vedere e in che ordine (trascinandoli), e salvi la barra in profili con un nome.

**Modalità zen** (il pulsante *Zen* in alto a destra, `Alt+Z` o `Ctrl+Maiusc+\`): tutto schermo, solo la lavagna; la barra degli strumenti compare avvicinandosi al bordo e si fa da parte mentre disegni; `Esc` per uscire.

**Timer, musica e votazione** (il disco e il tempo accanto a *Condividi*): il tempo si sceglie con − e + o tra le durate pronte (1, 3, 5, 10 minuti), oppure si scrive come su un microonde (solo cifre, da destra: `130` è 1:30, fino a 99 ore), e parte con Invio o col pulsante tondo per tutti; *+1 min*, pausa e stop; a 00:00 il tempo trema e suona un campanello. Il giradischi suona per tutti, allo stesso punto, uno dei sei brani integrati (Acoustic ambient, Lo-fi hip hop, Button smash, It's Friday, Synthwave, Hero's journey), composti e generati da Tratto stesso, senza file né diritti; il volume è di ognuno. *Nuova votazione* fa votare note, forme e immagini; alla fine un pannello a destra mostra la classifica (chi ospita o ha avviato la votazione la rifà o la toglie per tutti).

**Emote**: tieni premuto `E` (un tocco breve resta la gomma) e scegli un'emoji dalla ruota (tenendo premuto il clic ne parte una raffica), o dal menu accanto agli avatar: vola dal cursore e la vedono tutti. Le opzioni dello strumento sopra la barra si chiudono con la freccia, o cliccando di nuovo lo strumento, che resta in uso.

**Interazione contestuale.** Selezionando qualcosa compare sopra una barra scura con i suoi colori, la forma, il carattere e le altre impostazioni. Il **+** della barra degli strumenti aggiunge blocchi di codice e widget: con un clic in mezzo alla vista, trascinandoli dove li lasci (lo stesso per i modelli). *Connettori*: si attaccano al centro dei lati, dritti, a gomito o curvi; inizio e fine hanno la punta che vuoi (triangolo, freccia aperta, cerchio, rombo, barra) e la direzione si inverte con un clic; doppio clic su una linea per scriverci un'etichetta. *Widget*: si cliccano direttamente sulla lavagna (con lo strumento di selezione) e si scrivono nel pannello a destra.

**Diagrammi e mappe.** Passando sopra una forma o una nota compaiono quattro **+**: un clic aggiunge un nodo collegato da quel lato (cliccando ancora lo stesso **+** i nodi si affiancano, come i rami di una decisione), trascinandolo si tira una freccia. Con una forma o una nota selezionata (anche mentre ci scrivi): **Tab** aggiunge il nodo dopo, **Maiusc+Tab** uno accanto sullo stesso ramo, **Ctrl+frecce** uno in quella direzione. Le frecce seguono le forme quando le sposti; *Riordina* nella barra scura mette in ordine tutto il diagramma che parte da lì.

**Livelli.** Nel pannello a sinistra si trascina una riga per cambiare l'ordine (una linea viola mostra dove andrà): sopra o sotto gli altri elementi, dentro una cartella lasciandola sulla sua riga, fuori trascinandola fra gli elementi in cima; con più elementi selezionati si spostano tutti insieme, e `Esc` annulla. Gli elementi di una sezione si riordinano fra loro (fanno parte della sezione perché ci stanno dentro sulla lavagna).

**Tabelle.** Righe e colonne si scelgono nella barra dello strumento. Un clic seleziona la tabella, un altro clic su una cella ci scrive (trascinando la sposti); Tab passa alla cella dopo (nell'ultima aggiunge una riga). Mentre scrivi, una barretta sopra la tabella cambia il colore della cella e aggiunge o toglie righe e colonne; i **+** sotto e a destra le aggiungono in fondo; le linee tra le colonne si trascinano per allargarle. Nei *Livelli* la tabella si apre sulle sue celle (un clic e ci scrivi), le sezioni sui loro elementi, i widget sulle loro voci.

**Pannello Design.** I numeri si cambiano trascinando la loro etichetta (Maiusc per andare più veloce) o con le frecce mentre scrivi. Forme, note, tabelle e blocchi di codice hanno gli *angoli* arrotondabili (campo e cursore). Il testo di qualsiasi elemento (caselle, note, forme, tabelle, codice) ha carattere, dimensione, grassetto, corsivo, allineamento e colore; ogni tavolozza ha anche il colore personalizzato. Note, forme piene, tabelle, codice, widget e immagini hanno un'*ombra* regolabile (densità, sfumatura, distanza). La *minimappa* si sposta dove vuoi trascinandola dalla maniglia in alto a sinistra. Su una lavagna scura l'inchiostro nero di base si vede chiaro.

**Esportare.** *Menu ▸ Esporta*: PNG, JPG, SVG, PDF o file Tratto.

Tutte le scorciatoie sono nel menu, alla voce *Scorciatoie da tastiera* (`?`).

**Se Tratto non si apre.** Le schede video e i loro driver si guastano in modi diversi: un errore, un crash, o una finestra che non compare mai perché il driver si blocca. Tratto prova da solo, uno dopo l'altro, OpenGL, Direct3D 12, Vulkan e infine la grafica software di Windows (più lenta, ma c'è sempre), finché la finestra compare; quello che funziona viene ricordato per le volte successive. Se nessuno funziona compare un messaggio. Ogni avvio è annotato in `startup.log`, accanto alle lavagne (`%APPDATA%\Tratto` su Windows): è il file da mandare quando qualcosa non va. Per provare un modo preciso c'è la variabile `TRATTO_RENDERER` (`gl`, `dx12`, `vulkan`, `software`); `TRATTO_TRACE=file.csv` annota il tempo di ogni fotogramma, per capire dove rallenta.

**Aggiornamenti.** All'avvio Tratto controlla se c'è una versione nuova: se c'è, nella schermata delle lavagne compare *Aggiorna a X.Y.Z e riavvia*, che scarica, installa e riapre Tratto (su Windows l'installer senza domande; sul Mac l'app sostituita; su Linux l'AppImage sostituita, oppure il .rpm o il .deb installati con la password di sistema). Le lavagne non si toccano. Chi ha ancora Tratto 1 su Windows o in AppImage riceve la versione 2 da solo, alla chiusura dell'app.

## Per chi la sviluppa

Requisiti: Rust stabile, più `rustup target add wasm32-unknown-unknown` e `cargo install wasm-bindgen-cli --version 0.2.129` per la pagina degli ospiti.

```sh
sh scripts/web.sh           # la pagina degli ospiti (la stessa app in WebAssembly) in web/pkg
cargo run --release         # l'app; per condividere serve bin/cloudflared
cargo test
```

**Installer.** La GitHub Action `.github/workflows/build.yml` costruisce su Windows (NSIS), Mac (dmg arm64 e x64) e Linux (AppImage e deb) con [cargo-packager], più l'rpm con `cargo generate-rpm`. Su un tag `vX.Y.Z` pubblica una sola release con tutti i file, più `latest.yml` e `latest-linux.yml` che fanno aggiornare da soli i Tratto 1.x. Per una nuova versione: alza `version` in `Cargo.toml`, scrivi le note in `.github/notes/vX.Y.Z.md` (facoltative), crea il tag annotato `vX.Y.Z` e caricalo.

**Firma su Windows.** SmartScreen e gli antivirus trattano da sconosciuto un programma senza firma digitale. `scripts/sign.ps1` firma l'app, l'installer e il programma di disinstallazione (la build di Windows lo fa chiamare a cargo-packager per ognuno) appena il repository ha un certificato di firma del codice nei *Secrets* delle Actions: o un certificato in formato .pfx (`WINDOWS_CERTIFICATE` con il file in base64, `WINDOWS_CERTIFICATE_PASSWORD`) o Azure Trusted Signing (le variabili elencate nello script). Senza certificato la build resta uguale, non firmata. Un certificato autofirmato non serve: Windows non lo riconosce. Anche con un certificato vero, SmartScreen può avvisare nei primi giorni, finché il programma non si fa una reputazione; se Defender segnala per errore una versione, il file si invia come falso positivo dal portale dedicato per gli sviluppatori.

### Come è fatta

| Parte | File | Note |
|---|---|---|
| App | `src/main.rs`, `src/app/`, `src/diag.rs` | egui/eframe; schermata delle lavagne, lavagna, pannelli, finestre di dialogo. Su Windows parte con OpenGL (glow: la finestra in un decimo di secondo), poi Direct3D 12, Vulkan e WARP se serve; su Linux OpenGL sulla scheda integrata (Vulkan sveglierebbe la scheda dedicata: 2 secondi all'avvio, più batteria). |
| Penna | `src/pen.rs`, `src/ink.rs`, `src/brush.rs` | Penna e dito: Windows Ink, Wintab, gli eventi del browser. Forma dei tratti e dei pennelli, in pezzi da 64 punti (mentre scrivi si rifà solo l'ultimo); grana di matita e gessetto, macchie dell'aerografo. |
| Disegno | `src/paint.rs`, `src/prims.rs`, `src/text.rs` | Ogni elemento diventa una mesh (lyon) tenuta in cache; il testo usa rustybuzz con i font in `assets/fonts`. Con la lavagna rimpicciolita i tratti perdono i punti superflui e il testo troppo piccolo diventa una barretta. |
| Dati | `src/store.rs`, `src/doc.rs` | SQLite in `%APPDATA%\Tratto\tratto.db`, lavagne come stato Yjs (yrs): lo stesso formato della versione 1. |
| Condivisione | `src/share.rs`, `src/guest.rs` | Server locale (axum) raggiunto solo dal Cloudflare quick tunnel. Il codice di sessione sta dopo `#` e non finisce nei log; gli ospiti passano dalla sala d'attesa e ricevono un biglietto revocabile. Gli ospiti usano la stessa app compilata in WebAssembly. |

### Limiti noti
- Il link `trycloudflare.com` cambia a ogni condivisione. Per un indirizzo fisso serve un tunnel Cloudflare con nome e un dominio tuo.
- Tutti gli ospiti condividono un solo link per volta, legato a una sola lavagna.
- Delle funzioni collaborative non sono presenti quelle che richiedono un cloud centralizzato: l'intelligenza artificiale remota, i plugin esterni e i widget di terze parti. Le emoji 3D sono le Fluent Emoji (licenza MIT).

[egui]: https://github.com/emilk/egui
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager
