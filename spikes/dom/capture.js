/**
 * Capteur DOM in-page — banc de mesure du spike D20.
 *
 * Injecté AVANT les scripts de la page, dans toutes les frames. Écoute en phase
 * de capture sur `document` et lit `composedPath()[0]` : c'est la seule façon
 * d'atteindre la vraie cible à travers le shadow DOM des Lightning Web
 * Components, où `event.target` est reciblée sur l'hôte.
 *
 * Ne juge rien, ne filtre rien : il enregistre les ancrages candidats
 * séparément, et c'est l'orchestrateur qui compare les formules. Un capteur qui
 * choisit son ancrage avant la mesure répond à la question qu'on lui pose.
 *
 * Le coût est mesuré de l'intérieur : on chronomètre le temps passé DANS les
 * gestionnaires, rapporté au temps écoulé. C'est le surcoût in-page réel que la
 * page subit, pas une estimation externe.
 */
(() => {
  if (window.__noeCapture) return;

  let t0 = performance.now();
  const obs = [];
  let coutMs = 0;

  // --- Normalisation post-pipeline ---------------------------------------
  // Miroir exact de normaliser_nom() du binaire Rust (spikes/capteur-uia).
  // Toute divergence ici rendrait les deux spikes incomparables.
  const MOTIFS = [
    [/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, 'EMAIL'],
    [/(?:\+33|0)[1-9](?:[ .-]?\d{2}){4}/g, 'TEL'],
    [/\b[a-zA-Z0-9]{18}\b/g, 'ID18'],
    [/\b[a-zA-Z0-9]{15}\b/g, 'ID15'],
    [/il y a \d+\s*\w+/gi, 'TEMPS'],
    [/\d+\s*(minutes?|heures?|jours?|min|h|hours?|days?|ago)\b/gi, 'TEMPS'],
    [/\d{1,2}[/:]\d{2}(?:[/:]\d{2,4})?/g, 'TEMPS'],
    [/\(\s*\d+\s*\)/g, 'N'],
    [/\d{4,}/g, 'N'],
  ];

  const norm = (brut) => {
    let s = String(brut ?? '').trim();
    for (const [re, jeton] of MOTIFS) s = s.replace(re, jeton);
    return s.split(/\s+/).filter(Boolean).join(' ').toLowerCase();
  };

  // --- Rôle ---------------------------------------------------------------
  const ROLE_TAG = {
    A: 'link',
    BUTTON: 'button',
    SELECT: 'combobox',
    TEXTAREA: 'textbox',
    OPTION: 'option',
    SUMMARY: 'button',
    H1: 'heading',
    H2: 'heading',
    H3: 'heading',
    TABLE: 'table',
    TR: 'row',
    TD: 'cell',
    TH: 'columnheader',
    UL: 'list',
    OL: 'list',
    LI: 'listitem',
    IMG: 'img',
    FORM: 'form',
  };
  const ROLE_INPUT = {
    checkbox: 'checkbox',
    radio: 'radio',
    button: 'button',
    submit: 'button',
    reset: 'button',
    range: 'slider',
    search: 'searchbox',
    email: 'textbox',
    tel: 'textbox',
    url: 'textbox',
    number: 'spinbutton',
  };

  const roleDe = (el) => {
    const explicite = el.getAttribute?.('role');
    if (explicite?.trim()) {
      return { role: explicite.trim().split(/\s+/)[0].toLowerCase(), explicite: true };
    }
    const t = el.tagName;
    if (t === 'INPUT') {
      const ty = (el.getAttribute('type') || 'text').toLowerCase();
      return { role: ROLE_INPUT[ty] || 'textbox', explicite: false };
    }
    if (t === 'A') return { role: el.hasAttribute('href') ? 'link' : 'generic', explicite: false };
    return { role: ROLE_TAG[t] || 'generic', explicite: false };
  };

  // --- Nom accessible -----------------------------------------------------
  // Approximation de l'algorithme accname, dans son ordre de priorité. On
  // s'arrête au premier qui rend du texte : c'est ce que ferait un lecteur
  // d'écran, donc ce que verrait un capteur qui s'appuie sur la sémantique.
  const texteDe = (el) => {
    const t = (el.textContent || '').trim();
    return t.length > 120 ? t.slice(0, 120) : t;
  };

  const nomDe = (el) => {
    try {
      const par = el.getAttribute('aria-labelledby');
      if (par) {
        const racine = el.getRootNode();
        const bouts = par
          .split(/\s+/)
          .map((id) => {
            const c = racine.getElementById?.(id);
            return c ? texteDe(c) : '';
          })
          .filter(Boolean);
        if (bouts.length) return bouts.join(' ');
      }
      const lab = el.getAttribute('aria-label');
      if (lab?.trim()) return lab.trim();

      if (el.id) {
        const q = el.getRootNode().querySelector?.(`label[for="${CSS.escape(el.id)}"]`);
        if (q) return texteDe(q);
      }
      const enveloppe = el.closest?.('label');
      if (enveloppe) return texteDe(enveloppe);

      for (const a of ['title', 'placeholder', 'alt', 'name']) {
        const v = el.getAttribute(a);
        if (v?.trim()) return v.trim();
      }
      return texteDe(el);
    } catch {
      return '';
    }
  };

  // --- Ancrages data-* ----------------------------------------------------
  // On ne trie pas : on rend tout, valeurs normalisées. C'est l'analyse qui
  // dira quelles clés tiennent et lesquelles sont des identifiants de rendu.
  const dataDe = (el) => {
    const out = {};
    if (!el.attributes) return out;
    for (const a of el.attributes) {
      if (a.name.startsWith('data-')) out[a.name] = norm(a.value);
    }
    return out;
  };

  const CLES_TEST = ['data-testid', 'data-test-id', 'data-test', 'data-qa', 'data-cy'];

  const testidDe = (chemin) => {
    for (const el of chemin.slice(0, 8)) {
      if (!el.getAttribute) continue;
      for (const c of CLES_TEST) {
        const v = el.getAttribute(c);
        if (v?.trim()) return `${c}=${norm(v)}`;
      }
    }
    return '';
  };

  // --- Chemin structurel --------------------------------------------------
  // Construit depuis composedPath, donc traverse les shadow roots. On garde le
  // rang parmi les frères de même balise : c'est ce qui distingue deux boutons
  // identiques dans une même liste.
  const cranDe = (el) => {
    const p = el.parentNode;
    if (!p?.children) return 0;
    let n = 0;
    for (const f of p.children) {
      if (f.tagName === el.tagName) {
        n++;
        if (f === el) return n;
      }
    }
    return 0;
  };

  const cheminDe = (chemin) => {
    const bouts = [];
    for (const el of chemin) {
      if (bouts.length >= 5) break;
      if (!el?.tagName) continue;
      bouts.push(`${el.tagName.toLowerCase()}[${cranDe(el)}]`);
    }
    return bouts.join('>');
  };

  // --- Actions d'état -----------------------------------------------------
  // Un `input` n'en est pas une : il se déclenche à chaque frappe et gonflerait
  // la couverture sans qu'aucun état n'ait changé. Un `change`, un `submit`, et
  // un clic sur un rôle actionnable en sont.
  const ROLES_ACTION = new Set([
    'button',
    'link',
    'option',
    'checkbox',
    'radio',
    'tab',
    'switch',
    'menuitem',
    'menuitemcheckbox',
    'menuitemradio',
  ]);

  // Rôles sur lesquels on agit sans qu'ils changent l'état par eux-mêmes :
  // ouvrir une liste, poser le curseur. Ils font partie des acteurs possibles,
  // pas des actions d'état.
  const ROLES_ACTEUR = new Set([...ROLES_ACTION, 'combobox', 'textbox', 'searchbox', 'spinbutton']);

  /**
   * L'élément réellement actionné, pas le nœud sous le pixel.
   *
   * Dans un menu Lightning, la cible d'un clic est un `<span>` nu à l'intérieur
   * d'un `<li><a>`. Prise au pied de la lettre, elle a le rôle `generic` et
   * aucun nom : l'action qui change effectivement la valeur devient invisible.
   * Cliquer un span dans un lien, c'est cliquer le lien — on remonte donc le
   * chemin composé jusqu'au premier rôle sur lequel on peut agir.
   *
   * Ce n'est pas un arrangement avec la mesure : c'est la façon dont un humain
   * décrirait son geste, donc ce que le capteur doit enregistrer.
   */
  const acteurDe = (elements) => {
    for (const el of elements.slice(0, 10)) {
      const r = roleDe(el);
      if (ROLES_ACTEUR.has(r.role)) return { el, ...r };
    }
    const el = elements[0];
    return el ? { el, ...roleDe(el) } : null;
  };

  const estEtat = (type, role) => {
    if (type === 'change' || type === 'submit') return true;
    if (type === 'click') return ROLES_ACTION.has(role);
    return false;
  };

  // Un événement composé remonte jusqu'à `document` ET traverse chaque racine
  // shadow branchée : sans ce garde, il serait compté autant de fois qu'il
  // franchit de frontières.
  const VUS = new WeakSet();

  const observer = (type, ev) => {
    if (VUS.has(ev)) return;
    VUS.add(ev);
    const depart = performance.now();
    try {
      const chemin = ev.composedPath ? ev.composedPath() : [ev.target];
      const elements = chemin.filter((x) => x?.nodeType === 1);
      const cible = elements[0];
      const acteur = acteurDe(elements);
      if (!cible || !acteur) return;

      const brut = nomDe(acteur.el);
      const rangActeur = elements.indexOf(acteur.el);

      const o = {
        type,
        role: acteur.role,
        explicite: acteur.explicite,
        nom_brut: brut,
        nom: norm(brut),
        data: dataDe(acteur.el),
        testid: testidDe(elements),
        chemin: cheminDe(elements.slice(rangActeur < 0 ? 0 : rangActeur)),
        // La cible brute est conservée : l'écart entre elle et l'acteur mesure
        // ce que la remontée apporte, et permet de la contester.
        cible_role: roleDe(cible).role,
        cible_nom: norm(nomDe(cible)),
        remontees: rangActeur < 0 ? 0 : rangActeur,
        etat: estEtat(type, acteur.role),
        ms: Math.round(performance.now() - t0),
      };

      // Le coût porté par l'observation elle-même, et non par un compteur
      // global : un compteur global meurt avec le document à chaque navigation,
      // l'observation lui survit puisqu'elle part tout de suite.
      o.cout_ms = performance.now() - depart;
      coutMs += o.cout_ms;
      obs.push(o);

      // Sortie au fil de l'eau. Un tampon dans la page ne survit pas à une
      // navigation : la première phase large a perdu 100 % de ses observations
      // parce qu'elle relisait le tampon après trois `goto`. Une extension
      // réelle a le même problème et la même parade — le script de contenu
      // pousse vers le service worker à chaque événement, il n'accumule pas.
      //
      // Le coût du transport est volontairement HORS chronomètre : ici c'est un
      // aller-retour CDP, en production un `chrome.runtime.sendMessage`. Les
      // deux n'ont pas le même prix, et mesurer celui du banc tromperait sur
      // celui du produit.
      try {
        globalThis.__noePousser?.(o);
      } catch {
        /* pas de sortie branchée : le tampon local suffit */
      }
    } catch {
      // Un capteur qui jette casse la page qu'il observe. Jamais.
    }
  };

  const TYPES = ['click', 'change', 'submit', 'input'];
  const BRANCHEES = new WeakSet();

  const brancher = (cible) => {
    if (!cible || BRANCHEES.has(cible)) return;
    BRANCHEES.add(cible);
    for (const t of TYPES) cible.addEventListener(t, (ev) => observer(t, ev), true);
  };

  brancher(document);

  /**
   * `change` ne franchit PAS une frontière shadow.
   *
   * Contrairement à `input` et `click`, l'événement `change` est spécifié
   * `composed: false`. Émis par un `<select>` ou un `<input>` à l'intérieur d'un
   * Lightning Web Component, il meurt à la frontière de sa racine shadow et
   * n'atteint jamais `document`. Un capteur branché sur le seul document ne voit
   * donc AUCUN changement de valeur — c'est exactement ce que la première
   * mesure a montré : trente observations, trente clics, zéro `change`.
   *
   * On instrumente donc `attachShadow` pour brancher chaque racine dès sa
   * création. Le script d'init s'exécute avant tout script de la page : toutes
   * les racines des composants sont créées après nous, donc toutes sont vues.
   */
  const attachShadowNatif = Element.prototype.attachShadow;
  Element.prototype.attachShadow = function attachShadow(init) {
    const racine = attachShadowNatif.call(this, init);
    try {
      brancher(racine);
    } catch {
      /* jamais au prix de la page */
    }
    return racine;
  };

  window.__noeCapture = {
    lire: () => ({
      obs: obs.slice(),
      cout_ms: coutMs,
      ecoule_ms: performance.now() - t0,
      racines_branchees: true,
      url: String(location.href).slice(0, 120),
    }),
    vider: () => {
      obs.length = 0;
      coutMs = 0;
      t0 = performance.now();
    },
  };
})();
