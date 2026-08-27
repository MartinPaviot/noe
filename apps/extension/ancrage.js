/**
 * L'ancrage d'un élément : ce qui doit rester le même d'une exécution à l'autre.
 *
 * Extrait du spike DOM (`docs/spike-verdict-dom.md`), et pas réinventé : le nom
 * accessible normalisé tient 100 % de stabilité sur trois exécutions, et tout
 * enrichissement en bloc le dégrade. C'est mesuré, pas supposé.
 *
 * Ce module ne touche ni au réseau, ni au stockage, ni au presse-papiers. Il lit
 * l'arbre et rend des chaînes.
 */

/**
 * Les `data-*` admis dans l'ancrage — liste blanche SÉMANTIQUE, jamais en bloc.
 *
 * Le spike a mesuré la règle et désigné le coupable : `data-aura-rendered-by`
 * porte un identifiant de rendu (« 931:0;a ») qui change à chaque re-rendu
 * d'Aura, traverse la normalisation intact, et fait tomber la stabilité de
 * 100 % à 80 % à lui seul. `data-tab-value` et `data-label`, eux, tiennent 100 %.
 *
 * Un attribut dont la valeur est produite par le moteur de rendu est un poison
 * d'ancrage, pas un renfort. La liste s'allonge par la mesure, jamais par
 * l'intuition.
 */
export const DATA_ADMIS = ['data-label', 'data-tab-value', 'data-name', 'data-field'];

/**
 * Rôles ARIA implicites par balise.
 *
 * Le spike a compté : **la moitié seulement** des éléments actionnés porte un
 * rôle explicite. Sans déduction, un bouton sur deux serait ancré `generic|…` et
 * les branches de la spec 004 ne se rejoindraient pas.
 *
 * Le vocabulaire est celui d'ARIA, comme côté UIA — c'est ce qui permet au même
 * contrôle d'avoir le même ancrage quelle que soit la source qui l'a vu.
 */
const ROLE_PAR_BALISE = {
  a: 'link',
  button: 'button',
  input: 'textbox',
  select: 'combobox',
  textarea: 'textbox',
  form: 'form',
  table: 'table',
  ul: 'list',
  ol: 'list',
  li: 'listitem',
  nav: 'navigation',
  h1: 'heading',
  h2: 'heading',
  h3: 'heading',
  h4: 'heading',
  h5: 'heading',
  h6: 'heading',
  img: 'img',
  dialog: 'dialog',
};

/** Les `type` d'`input` qui ne sont pas des zones de texte. */
const ROLE_PAR_TYPE = {
  button: 'button',
  submit: 'button',
  reset: 'button',
  checkbox: 'checkbox',
  radio: 'radio',
  range: 'slider',
  search: 'searchbox',
};

/**
 * Les champs dont on ne lit RIEN, pas même le nom.
 *
 * Un champ de mot de passe n'a pas d'intérêt d'ancrage et beaucoup de risque : le
 * navigateur y met parfois le nom du compte en `aria-label`, et le gestionnaire
 * de mots de passe y écrit des attributs qui portent l'identité. On s'arrête
 * avant, plutôt que de compter sur la rédaction en aval pour rattraper.
 */
export function estSensible(el) {
  if (!el || el.nodeType !== 1) return false;
  const balise = el.tagName?.toLowerCase();
  if (balise !== 'input') return false;
  const type = (el.getAttribute('type') || 'text').toLowerCase();
  return type === 'password' || el.autocomplete === 'current-password';
}

export function roleDe(el) {
  const explicite = el.getAttribute?.('role');
  if (explicite) return explicite.trim().toLowerCase().split(/\s+/)[0];

  const balise = el.tagName?.toLowerCase() ?? '';
  if (balise === 'input') {
    const type = (el.getAttribute('type') || 'text').toLowerCase();
    return ROLE_PAR_TYPE[type] ?? 'textbox';
  }
  return ROLE_PAR_BALISE[balise] ?? 'generic';
}

/**
 * Normalise un nom accessible.
 *
 * Même normalisation que côté natif : blancs réduits, bornes coupées. Deux
 * sources qui normaliseraient différemment produiraient deux ancrages pour le
 * même contrôle, et l'épisode mixte de R2.1 se lirait comme deux tâches.
 */
export function normaliser(texte) {
  return (texte ?? '').replace(/\s+/g, ' ').trim();
}

/**
 * Le nom accessible, dans l'ordre de priorité d'ARIA.
 *
 * `aria-labelledby` d'abord parce que c'est ce que fait le navigateur, et qu'un
 * ancrage qui diverge de ce que l'utilisateur entend n'ancre pas ce qu'il croit.
 */
export function nomAccessibleDe(el) {
  const racine = el.getRootNode?.() ?? document;

  const parId = el.getAttribute?.('aria-labelledby');
  if (parId) {
    const morceaux = parId
      .split(/\s+/)
      .map((id) => racine.getElementById?.(id)?.textContent ?? '')
      .filter(Boolean);
    if (morceaux.length > 0) return normaliser(morceaux.join(' '));
  }

  const direct = el.getAttribute?.('aria-label');
  if (direct) return normaliser(direct);

  if (el.id) {
    const etiquette = racine.querySelector?.(`label[for="${CSS.escape(el.id)}"]`);
    if (etiquette?.textContent) return normaliser(etiquette.textContent);
  }

  const enveloppe = el.closest?.('label');
  if (enveloppe?.textContent) return normaliser(enveloppe.textContent);

  const titre = el.getAttribute?.('title');
  if (titre) return normaliser(titre);

  const alt = el.getAttribute?.('alt');
  if (alt) return normaliser(alt);

  const place = el.getAttribute?.('placeholder');
  if (place) return normaliser(place);

  // Le texte n'est retenu que s'il est court : le contenu entier d'un conteneur
  // n'est pas un nom, c'est une page, et il porterait tout ce qu'elle affiche.
  const texte = normaliser(el.textContent ?? '');
  return texte.length > 0 && texte.length <= 120 ? texte : '';
}

/**
 * Le parent, **frontière shadow franchie**.
 *
 * `parentElement` rend `null` à la racine d'une racine shadow : le parent réel
 * est l'hôte. Sans ce saut, mesuré sur le banc, tout contrôle vivant dans un
 * composant Lightning sortait avec un chemin **vide** et une région **nulle** —
 * c'est-à-dire sans départage et sans contexte, exactement ce que l'ancrage doit
 * fournir.
 */
function parentReel(el) {
  if (el.parentElement) return el.parentElement;
  const racine = el.getRootNode?.();
  return racine instanceof ShadowRoot ? racine.host : null;
}

/**
 * Le conteneur nommé le plus proche, frontières shadow comprises.
 *
 * `closest` s'arrête à la racine shadow. Le remonter à la main est le seul moyen
 * de retrouver la région d'un champ enfermé dans son composant.
 */
function conteneurNomme(el) {
  const SELECTEUR =
    '[role="region"],[role="dialog"],[role="tabpanel"],[role="form"],section,form,dialog';
  let courant = parentReel(el);
  for (let i = 0; i < 24 && courant; i += 1) {
    // On ne s'arrête qu'à un conteneur qui a un NOM. Mesuré sur le banc : le
    // bouton « Enregistrer » vit dans un `<form>` anonyme, et s'arrêter là lui
    // donnait `region: null` alors que la section qui l'englobe s'appelle
    // « Details de la piste ». Un conteneur sans nom ne situe rien ; il n'a
    // aucune raison d'arrêter la remontée.
    if (courant.matches?.(SELECTEUR) && nomExplicite(courant).length > 0) return courant;
    courant = parentReel(courant);
  }
  return null;
}

/**
 * Le nom d'un conteneur : explicitement déclaré, ou rien.
 *
 * Pas de repli sur le texte. Mesuré sur le banc : un `<form>` qui ne contient
 * qu'un bouton « Enregistrer » ressortait avec la région « Enregistrer ». Une
 * région nommée d'après son unique contrôle n'ajoute pas de contexte, elle en
 * invente.
 */
function nomExplicite(el) {
  if (el.getAttribute?.('aria-labelledby')) return nomAccessibleDe(el);
  return normaliser(el.getAttribute?.('aria-label') ?? el.getAttribute?.('title') ?? '');
}

/**
 * Le chemin structurel, en départage.
 *
 * L'ancrage principal est `rôle | nom`. Le chemin ne sert qu'à distinguer deux
 * contrôles homonymes — cinq boutons « Modifier » sur une même fiche — et il est
 * volontairement **court** : un chemin complet depuis la racine change au premier
 * conteneur inséré par le framework, ce qui en ferait un poison d'ancrage de la
 * même famille que `data-aura-rendered-by`.
 *
 * Il ne porte que des noms de balise et des index de fratrie : jamais de texte,
 * donc jamais de PII.
 */
export function cheminDe(el, profondeur = 4) {
  const morceaux = [];
  let courant = el;
  for (let i = 0; i < profondeur && courant && courant.nodeType === 1; i += 1) {
    const parent = parentReel(courant);
    if (!parent) break;
    const fratrie = courant.parentElement
      ? [...courant.parentElement.children]
      : [...(courant.getRootNode()?.children ?? [])];
    const memeBalise = fratrie.filter((c) => c.tagName === courant.tagName);
    const rang = memeBalise.indexOf(courant);
    morceaux.unshift(`${courant.tagName.toLowerCase()}[${rang < 0 ? 0 : rang}]`);
    courant = parent;
  }
  return morceaux.join('/');
}

/**
 * La région : le conteneur nommé le plus proche.
 *
 * **Uniquement un nom explicite** — `aria-label`, `aria-labelledby`, `title`.
 * Pas de repli sur le texte : mesuré sur le banc, un `<form>` qui ne contient
 * qu'un bouton « Enregistrer » ressortait avec la région « Enregistrer ». Une
 * région nommée d'après son unique contrôle ne situe rien ; elle ajoute du bruit
 * là où l'ancrage a besoin de contexte.
 */
export function regionDe(el) {
  const conteneur = conteneurNomme(el);
  if (!conteneur || conteneur === el) return null;
  const nom = nomExplicite(conteneur);
  return nom.length > 0 ? nom : null;
}

/** Les `data-*` retenus, dans un ordre stable. */
export function dataAdmisDe(el) {
  const retenus = {};
  for (const cle of DATA_ADMIS) {
    const v = el.getAttribute?.(cle);
    if (v) retenus[cle] = normaliser(v);
  }
  return retenus;
}

/**
 * L'observation complète d'un élément.
 *
 * Rend `null` pour un élément sensible : c'est le seul cas où l'on préfère ne
 * rien voir. **La valeur d'un champ n'est jamais lue** — ni ici, ni ailleurs
 * dans ce script. Un changement de valeur se signale par le fait qu'il a eu
 * lieu, pas par ce qu'il vaut.
 */
export function cibleDe(el) {
  if (!el || el.nodeType !== 1 || estSensible(el)) return null;
  return {
    role: roleDe(el),
    nom: nomAccessibleDe(el),
    region: regionDe(el),
    chemin: cheminDe(el),
    data: dataAdmisDe(el),
  };
}
