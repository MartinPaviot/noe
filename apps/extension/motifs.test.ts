/**
 * La troisième implémentation lit-elle la bibliothèque comme les deux autres ?
 *
 * Le design l'exige : « une divergence entre les trois implémentations rendrait
 * les mesures incomparables et les canaris menteurs ». Rust est comparé à
 * TypeScript par `motifs.rs` ; ce banc-ci ferme le triangle.
 *
 * Il compare les **sorties sur des entrées partagées**, jamais les chaînes de
 * motifs — deux moteurs peuvent lire la même chaîne différemment, et c'est
 * précisément ce que la v3 a découvert quand Rust a refusé une anticipation que
 * JavaScript acceptait.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error — module JavaScript d'extension, sans types.
import { chercher, elider, normaliserBlancs, resoudreChevauchements } from './motifs.js';

const RACINE = join(import.meta.dirname, '..', '..');
const MIROIR = JSON.parse(
  readFileSync(join(RACINE, 'packages', 'episode-spec', 'motifs.json'), 'utf8'),
);
const VECTEURS = JSON.parse(
  readFileSync(join(RACINE, 'packages', 'episode-spec', 'vecteurs-redaction.json'), 'utf8'),
);

type Occurrence = { type: string; index: number; fin: number };
type Cas = { entree: string; occurrences: Occurrence[]; retenues: Occurrence[] };

describe('la troisieme implementation des motifs (extension)', () => {
  it('lit la meme version de la bibliotheque', () => {
    expect(MIROIR.version).toBe(VECTEURS.version);
  });

  it('voit exactement ce que voient les vecteurs partages', () => {
    const desaccords: string[] = [];
    for (const cas of VECTEURS.cas as Cas[]) {
      const obtenu = chercher(cas.entree, MIROIR).map((o: Occurrence) => [o.type, o.index]);
      const attendu = cas.occurrences.map((o) => [o.type, o.index]);
      if (JSON.stringify(obtenu) !== JSON.stringify(attendu)) {
        desaccords.push(
          `${JSON.stringify(cas.entree)}\n  attendu ${JSON.stringify(attendu)}\n  obtenu  ${JSON.stringify(obtenu)}`,
        );
      }
    }
    expect(desaccords, desaccords.join('\n')).toEqual([]);
  });

  it('arbitre les chevauchements exactement comme les deux autres', () => {
    // La detection seule ne suffit pas : c'est l'ARBITRAGE qui determine ce qui
    // sera remplace, donc les jetons, donc les jointures. Un test croise qui
    // ne comparait que la detection a deja laisse passer une divergence.
    const desaccords: string[] = [];
    for (const cas of VECTEURS.cas as Cas[]) {
      const brutes = chercher(cas.entree, MIROIR);
      const obtenu = resoudreChevauchements(brutes, MIROIR).map((o: Occurrence) => [
        o.type,
        o.index,
        o.fin,
      ]);
      const attendu = cas.retenues.map((o) => [o.type, o.index, o.fin]);
      if (JSON.stringify(obtenu) !== JSON.stringify(attendu)) {
        desaccords.push(
          `${JSON.stringify(cas.entree)}\n  attendu ${JSON.stringify(attendu)}\n  obtenu  ${JSON.stringify(obtenu)}`,
        );
      }
    }
    expect(desaccords, desaccords.join('\n')).toEqual([]);
  });

  it('normalise les blancs comme les deux autres', () => {
    expect(normaliserBlancs('06 12')).toBe('06 12');
    expect(normaliserBlancs('06 12')).toBe('06 12');
    // Largeur nulle : une espace et non rien, sinon `06<ZWSP>12` se lirait
    // `0612` et on inventerait une graphie que personne n'a ecrite.
    expect(normaliserBlancs('06​12')).toBe('06 12');
  });

  it('elide sans transporter la valeur', () => {
    // Ce que l'extension envoie n'est pas un jeton — produire un jeton
    // demanderait la cle du poste, et faire entrer cette cle dans une page web
    // serait absurde. Le TYPE suffit a garder l'ancrage stable.
    const elide = elider('Rappeler Jean au 06 12 34 56 78', MIROIR);
    expect(elide).toBe('Rappeler Jean au [TEL_FR]');
    expect(elide).not.toContain('06');
  });

  it('elide les graphies que la v4 a fermees', () => {
    for (const texte of [
      'Mobile +33 (0)6 12 34 56 78',
      'Depuis l etranger 0033 6 12 34 56 78',
      'Ligne 06 12 34 56 78',
    ]) {
      expect(elider(texte, MIROIR), texte).toContain('[TEL_FR]');
    }
  });

  it('ne mord pas sur ce qui n est pas une PII', () => {
    for (const texte of [
      'Reference interne 2026-08-26',
      'Version 1.2.3 du connecteur',
      'Code postal 75011 Paris',
      'Enregistrer',
    ]) {
      expect(elider(texte, MIROIR), texte).toBe(texte);
    }
  });
});
