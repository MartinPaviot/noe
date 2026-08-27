/**
 * Le service worker : le seul point de sortie de l'extension.
 *
 * Il ne connaît qu'une destination — le port de native messaging vers
 * l'application Noe, sur le même poste. **Aucun `fetch`, aucun `XMLHttpRequest`,
 * aucune socket.** C'est la première des cinq règles, tenue par construction et
 * pas par vigilance : il n'y a rien d'autre à quoi parler.
 *
 * Il numérote ce qu'il transmet. Un service worker MV3 est arrêté et relancé par
 * le navigateur quand il lui plaît ; la numérotation permet à l'application de
 * voir la discontinuité et de l'écrire comme un trou, au lieu de recoller deux
 * moitiés d'épisode en silence.
 */

const HOTE = 'app.noe.pont';

/** Le port, quand il est ouvert. Rouvert à la demande. */
let port = null;
/** Numéro de la prochaine observation. Repart à zéro si le worker redémarre. */
let seq = 0;
/** L'identifiant de vie du worker : c'est lui qui rend la coupure visible. */
const vie = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

function ouvrir() {
  if (port) return port;
  try {
    port = chrome.runtime.connectNative(HOTE);
  } catch (e) {
    console.error('[noe] pont indisponible :', e);
    port = null;
    return null;
  }
  port.onDisconnect.addListener(() => {
    // `lastError` porte la raison — hôte absent, manifeste non enregistré. On la
    // dit : une extension muette qui n'observe rien est indistinguable d'une
    // page sans activité.
    const raison = chrome.runtime.lastError?.message ?? 'deconnexion';
    console.warn('[noe] pont ferme :', raison);
    port = null;
  });
  return port;
}

chrome.runtime.onMessage.addListener((message, expediteur) => {
  if (message?.type !== 'observation') return;
  const p = ouvrir();
  if (!p) return;

  seq += 1;
  try {
    p.postMessage({
      ...message,
      seq,
      vie,
      // L'onglet, pas son titre : un titre d'onglet porte le nom du client.
      onglet: expediteur?.tab?.id ?? null,
      cadre: expediteur?.frameId ?? 0,
    });
  } catch (e) {
    console.warn('[noe] observation non transmise :', e);
    port = null;
  }
});

// Le pont s'ouvre au démarrage plutôt qu'à la première observation : si l'hôte
// n'est pas enregistré, on veut le savoir tout de suite, pas au premier clic.
ouvrir();
