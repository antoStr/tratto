import init from './pkg/tratto.js';

init().catch((e) => {
  console.error(e);
  document.getElementById('loading').textContent = 'Questo browser non riesce ad aprire la lavagna. Prova con Chrome, Edge, Firefox o Safari aggiornati.';
});
