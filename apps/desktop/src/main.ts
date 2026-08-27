/**
 * Le squelette traversant (D26) — la première vue.
 *
 * Une seule vue, branchée sur les vrais épisodes du poste : la liste, le grade
 * avec sa raison, la complétude, et la frise des événements et des trous.
 *
 * Elle naît maintenant et grandira à chaque spec, au lieu d'attendre la 008.
 * Le pari est simple : un fondateur qui voit ce qui se capture peut corriger le
 * tir ; un fondateur qui lit des tests verts ne le peut pas.
 */
import { detailEpisode, listerEpisodes, type PointFrise, type ResumeEpisode } from './ipc.js';

const RACINE_ID = 'app';

const echapper = (t: string): string =>
  t.replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] ?? c,
  );

/** `2026-01-01T09:12:03.000Z` → `01/01 09:12`. Une frise n'a pas besoin de plus. */
function bref(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getUTCDate())}/${p(d.getUTCMonth() + 1)} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}`;
}

function dureeBreve(t0: string, t1: string): string {
  const ms = new Date(t1).getTime() - new Date(t0).getTime();
  if (!Number.isFinite(ms) || ms < 0) return '—';
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

function carteEpisode(e: ResumeEpisode): string {
  return `
  <article class="episode" data-id="${echapper(e.id)}" tabindex="0">
    <header>
      <span class="grade grade-${echapper(e.grade)}">${echapper(e.grade)}</span>
      <span class="slug">${echapper(e.task_slug)}</span>
      <span class="quand">${echapper(bref(e.t0))} · ${echapper(dureeBreve(e.t0, e.t1))}</span>
    </header>
    <p class="raison">${echapper(e.grade_reason)}</p>
    <dl class="chiffres">
      <div><dt>actions</dt><dd>${e.actions}</dd></div>
      <div><dt>trous</dt><dd class="${e.trous > 0 ? 'alerte' : ''}">${e.trous}</dd></div>
      <div><dt>complétude</dt><dd>${e.completude_pct} %</dd></div>
      ${
        // R5.4 : silencieux quand il n'y a rien a dire. Une colonne « 0 » a
        // chaque carte apprendrait a l'oeil a ne plus la lire, et c'est
        // precisement le chiffre qu'il ne faut pas rater.
        e.hors_perimetre > 0
          ? `<div><dt>hors périmètre</dt><dd class="alerte hors">${e.hors_perimetre}</dd></div>`
          : ''
      }
    </dl>
    <p class="perimetre">${e.scope_fields.map((c) => `<span>${echapper(c)}</span>`).join('')}</p>
    <div class="frise" data-frise></div>
  </article>`;
}

function pointFrise(p: PointFrise): string {
  const titre =
    p.genre === 'trou'
      ? `trou : ${p.quoi}`
      : `${p.quoi} — ${p.cible}${p.region ? ` (${p.region})` : ''}`;
  return `<span class="pt pt-${echapper(p.genre)}" title="${echapper(titre)}">
    <b>${echapper(p.quoi)}</b><i>${echapper(p.cible)}</i></span>`;
}

async function garnirFrise(carte: HTMLElement): Promise<void> {
  const cible = carte.querySelector('[data-frise]');
  const id = carte.dataset['id'];
  if (!(cible instanceof HTMLElement) || id === undefined) return;
  if (cible.dataset['garnie'] === 'oui') return;

  // Un detail illisible ne doit pas figer la vue entiere : la liste est deja
  // affichee et reste utilisable, la frise manquante le dit.
  let detail: Awaited<ReturnType<typeof detailEpisode>> = null;
  try {
    detail = await detailEpisode(id);
  } catch {
    cible.dataset['garnie'] = 'oui';
    cible.innerHTML = '<span class="pt pt-vide">frise indisponible</span>';
    return;
  }

  cible.dataset['garnie'] = 'oui';
  cible.innerHTML =
    detail === null || detail.frise.length === 0
      ? '<span class="pt pt-vide">aucun evenement</span>'
      : detail.frise.map(pointFrise).join('');
}

function gabarit(contenu: string, sous_titre: string): string {
  return `
  <header class="tete">
    <h1>Noe</h1>
    <p>${echapper(sous_titre)}</p>
  </header>
  ${contenu}`;
}

const VIDE = `
  <section class="etat etat-vide">
    <h2>Aucun episode pour l'instant</h2>
    <p>Choisissez une tache active dans le menu de la barre d'etat, puis
       <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> pour commencer a observer.</p>
    <p class="apaise">C'est l'etat normal du premier jour.</p>
  </section>`;

const CHARGEMENT = `
  <section class="etat etat-chargement" aria-busy="true">
    <div class="squelette"></div>
    <div class="squelette"></div>
    <div class="squelette"></div>
  </section>`;

const erreur = (message: string): string => `
  <section class="etat etat-erreur" role="alert">
    <h2>Les episodes n'ont pas pu etre lus</h2>
    <p class="detail">${echapper(message)}</p>
    <p class="apaise">Rien n'est perdu : les episodes sont sur le disque.
       Ouvrez le dossier de donnees depuis le menu pour les voir.</p>
  </section>`;

async function monter(): Promise<void> {
  const racine = document.getElementById(RACINE_ID);
  if (racine === null) throw new Error(`Element #${RACINE_ID} introuvable`);

  racine.innerHTML = gabarit(CHARGEMENT, 'lecture des episodes…');

  let episodes: readonly ResumeEpisode[];
  try {
    episodes = await listerEpisodes();
  } catch (e) {
    racine.innerHTML = gabarit(
      erreur(e instanceof Error ? e.message : String(e)),
      'quelque chose ne va pas',
    );
    return;
  }

  if (episodes.length === 0) {
    racine.innerHTML = gabarit(VIDE, 'rien a montrer, pour l instant');
    return;
  }

  const pluriel = episodes.length > 1 ? 's' : '';
  racine.innerHTML = gabarit(
    `<section class="liste">${episodes.map(carteEpisode).join('')}</section>`,
    `${episodes.length} episode${pluriel} capture${pluriel}`,
  );

  // Les frises se garnissent après coup : la liste doit s'afficher tout de
  // suite, même si le détail de vingt épisodes prend un instant.
  //
  // `allSettled` et non `all` : une seule frise en échec ne doit pas empêcher
  // la vue de se déclarer prête. C'est un test visuel qui l'a montré — la vue
  // restait indéfiniment sans `data-pret`, donc invisible au contrôle.
  await Promise.allSettled(
    [...racine.querySelectorAll<HTMLElement>('.episode')].map((c) => garnirFrise(c)),
  );
  racine.dataset['pret'] = 'oui';
}

void monter();
