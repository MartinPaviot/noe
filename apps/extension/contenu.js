/**
 * Le script de contenu : ce qui observe, dans la page.
 *
 * Trois contraintes viennent du spike DOM et ne sont pas négociables.
 *
 * 1. **Il pousse, il n'accumule pas.** Un tampon in-page ne survit pas à une
 *    navigation : la première phase du spike a perdu 100 % de ses observations
 *    pour cette raison. Chaque observation part vers le service worker au fil de
 *    l'eau.
 *
 * 2. **Les racines shadow se branchent par balayage**, pas par patch
 *    d'`attachShadow` : Salesforce réassigne `Element.prototype.attachShadow`
 *    après tout script d'init, et le patch ne tient pas. Les 270 racines de
 *    Lightning sont toutes ouvertes, donc énumérables ; 411 racines branchées
 *    coûtent 10,6 ms.
 *
 * 3. **`change` est `composed: false`.** Il ne franchit aucune frontière shadow.
 *    Un capteur branché sur le seul `document` ne verra jamais un changement de
 *    valeur — le point 2 n'est donc pas une optimisation, c'est la condition
 *    d'existence de la fonction.
 *
 * Ce script ne lit jamais la valeur d'un champ, ne touche pas au presse-papiers,
 * et n'ouvre aucune connexion. Il parle à son service worker, qui parle à
 * l'application locale. Rien ne sort du poste.
 */
import { cibleDe, estSensible } from './ancrage.js';

/** Au-delà, on cesse de rebalayer : une page qui mute sans fin ne se calme pas. */
const BALAYAGES_MAX_PAR_SECONDE = 4;
/** Un changement de valeur par élément et par fenêtre : la frappe est continue. */
const ANTIREBOND_VALEUR_MS = 300;

const racinesBranchees = new WeakSet();
let dernierBalayage = 0;
let balayagePrevu = false;
const derniereValeur = new WeakMap();

/** L'horloge de la page ne sert qu'à l'antirebond ; l'épisode est daté par l'app. */
const maintenant = () => globalThis.performance?.now?.() ?? Date.now();

/**
 * Cet événement a-t-il déjà été observé ?
 *
 * Le document ET chaque racine shadow sont branchés, et un `click` est
 * `composed: true` : il traverse les frontières, donc le même geste réveille
 * plusieurs de nos écouteurs. Mesuré sur le banc — **chaque clic et chaque focus
 * arrivaient en double**, et l'épisode aurait compté deux actions là où
 * l'opérateur en a fait une.
 *
 * Le marqueur est posé sur l'objet `Event` lui-même : c'est le MÊME objet qui se
 * propage, donc le seul repère fiable. Un antirebond temporel confondrait deux
 * clics rapides sur le même bouton, ce qui est un geste réel.
 *
 * `submit` et `change` sont `composed: false` et n'arrivaient qu'une fois ; le
 * marqueur ne leur coûte rien et les protège d'un futur branchement.
 */
function dejaVu(evenement) {
  if (evenement.__noeVu) return true;
  try {
    Object.defineProperty(evenement, '__noeVu', { value: true, enumerable: false });
  } catch {
    // Un événement gelé : on préfère un doublon à une observation perdue.
    return false;
  }
  return false;
}

function pousser(genre, cible, extra) {
  if (cible === null) return;
  try {
    chrome.runtime.sendMessage({
      type: 'observation',
      genre,
      cible,
      url_origine: location.origin,
      ...extra,
    });
  } catch {
    // Le service worker peut être en train de redémarrer, ou l'extension d'être
    // rechargée. Une observation perdue est un trou ; il sera déclaré côté
    // application par la discontinuité de séquence, pas rattrapé ici par un
    // tampon qui ne survivrait pas à la navigation.
  }
}

function surClic(evenement) {
  if (dejaVu(evenement)) return;
  const el = evenement.composedPath?.()[0] ?? evenement.target;
  if (!(el instanceof Element)) return;
  // Le contrôle actionnable le plus proche, pas le nœud exact : un clic sur le
  // `<span>` d'un bouton est un clic sur le bouton.
  //
  // **Un champ de texte n'est PAS actionnable.** Mesuré sur le banc : cliquer
  // dans une zone de saisie produisait une `invocation` en plus du `focus`,
  // c'est-à-dire une action que l'opérateur n'a pas faite. Une case à cocher, un
  // bouton radio, un bouton d'envoi, si — le clic EST le geste.
  const actionnable =
    el.closest?.(
      'button,a,[role="button"],[role="link"],[role="menuitem"],[role="tab"],' +
        '[role="checkbox"],[role="radio"],[role="switch"],[role="option"],' +
        'input[type="button"],input[type="submit"],input[type="reset"],' +
        'input[type="checkbox"],input[type="radio"]',
    ) ?? null;
  if (actionnable === null) return;
  pousser('invocation', cibleDe(actionnable));
}

function surFocus(evenement) {
  if (dejaVu(evenement)) return;
  const el = evenement.composedPath?.()[0] ?? evenement.target;
  if (!(el instanceof Element)) return;
  pousser('focus', cibleDe(el));
}

/**
 * Un changement de valeur — **le fait, jamais la valeur**.
 *
 * On compare la longueur et non le contenu : « le champ est passé de vide à
 * rempli » suffit à l'ancrage, et rien de ce que l'opérateur a tapé ne traverse
 * la frontière.
 */
function surValeur(evenement) {
  if (dejaVu(evenement)) return;
  const el = evenement.composedPath?.()[0] ?? evenement.target;
  if (!(el instanceof Element) || estSensible(el)) return;

  const t = maintenant();
  const precedent = derniereValeur.get(el);
  if (precedent !== undefined && t - precedent.t < ANTIREBOND_VALEUR_MS) return;

  const longueur = typeof el.value === 'string' ? el.value.length : 0;
  const avant = precedent?.longueur ?? null;
  derniereValeur.set(el, { t, longueur });

  pousser('changement_valeur', cibleDe(el), {
    // Deux booléens, pas un contenu. C'est tout ce dont l'assemblage a besoin
    // pour dire qu'un champ a ete touche.
    etait_vide: avant === null ? null : avant === 0,
    est_vide: longueur === 0,
  });
}

function surSoumission(evenement) {
  if (dejaVu(evenement)) return;
  const el = evenement.composedPath?.()[0] ?? evenement.target;
  if (!(el instanceof Element)) return;
  pousser('soumission', cibleDe(el));
}

/** Branche une racine — le document, ou une racine shadow ouverte. */
function brancher(racine) {
  if (!racine || racinesBranchees.has(racine)) return;
  racinesBranchees.add(racine);
  // `capture: true` : on veut voir l'événement avant que la page ne l'arrête.
  // `passive: true` : on n'appelle jamais `preventDefault`, et le dire permet au
  // navigateur de ne pas attendre notre retour — c'est une partie du budget CPU.
  const options = { capture: true, passive: true };
  racine.addEventListener('click', surClic, options);
  racine.addEventListener('focusin', surFocus, options);
  racine.addEventListener('change', surValeur, options);
  racine.addEventListener('input', surValeur, options);
  racine.addEventListener('submit', surSoumission, options);
}

/**
 * Balaye les racines shadow OUVERTES et les branche.
 *
 * Le spike a démenti l'hypothèse inverse : sur Lightning, 270 éléments
 * personnalisés, 270 racines ouvertes, 0 fermée. Ce qui ne marche pas, c'est le
 * patch d'`attachShadow` — la page le réassigne après nous.
 */
function balayer(racine = document) {
  let branchees = 0;
  const parcourir = (noeud) => {
    const arbre = noeud.querySelectorAll?.('*') ?? [];
    for (const el of arbre) {
      const shadow = el.shadowRoot;
      if (shadow && !racinesBranchees.has(shadow)) {
        brancher(shadow);
        branchees += 1;
        parcourir(shadow);
      }
    }
  };
  parcourir(racine);
  return branchees;
}

/** Rebalaye, mais pas plus que quatre fois par seconde. */
function planifierBalayage() {
  if (balayagePrevu) return;
  const t = maintenant();
  const attente = Math.max(0, 1000 / BALAYAGES_MAX_PAR_SECONDE - (t - dernierBalayage));
  balayagePrevu = true;
  setTimeout(() => {
    balayagePrevu = false;
    dernierBalayage = maintenant();
    balayer();
  }, attente);
}

function demarrer() {
  brancher(document);
  balayer();

  // Rebalayage sur mutation : une racine shadow créée après le chargement doit
  // être branchée, sinon les contrôles qu'elle contient n'existent pas pour nous.
  const observateur = new MutationObserver(planifierBalayage);
  observateur.observe(document.documentElement ?? document, {
    childList: true,
    subtree: true,
  });

  // Une navigation dans une application monopage ne recharge pas le script :
  // les racines de la vue suivante sont neuves.
  globalThis.addEventListener?.('popstate', planifierBalayage);
  globalThis.addEventListener?.('hashchange', planifierBalayage);
}

// `document_start` : `documentElement` existe, `body` pas encore. On branche ce
// qui existe et le reste vient par mutation.
demarrer();

// Exporté pour le banc, jamais utilisé par la page.
export { balayer, brancher, surValeur };
