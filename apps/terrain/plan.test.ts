import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  adresse,
  CHAMPS_CANARIS,
  champsDeTousLesPerimetres,
  DOMAINE,
  plan,
  TACHES,
} from './plan.mjs';

/**
 * Le plan de terrain se vérifie **sans org** — c'est tout l'intérêt de l'avoir
 * séparé de son exécution.
 */

const CANARIS = 'packages/harness/golden/canaris.json';
const EXEMPLE = 'docs/terrain.example.json';

describe('le plan de terrain', () => {
  it('est deterministe — sinon peupler deux fois creerait des doublons', () => {
    expect(plan()).toEqual(plan());
  });

  it('n utilise que des champs standard', () => {
    // Aucun champ personnalise : les creer est une etape de configuration qui
    // peut echouer, et le jalon de la spec 003 ne doit pas dependre d'elle.
    const champs = plan().enregistrements.flatMap((e) => Object.keys(e.champs));
    const perso = champs.filter((c) => c.endsWith('__c'));
    expect(perso).toEqual([]);
    for (const t of Object.values(TACHES)) {
      expect(t.scope_fields.filter((c) => c.endsWith('__c'))).toEqual([]);
    }
  });

  it('n ecrit aucune adresse capable d atteindre quelqu un', () => {
    // RFC 2606 : `.invalid` n'est resolu par personne. Un jeu de demonstration
    // qui enverrait un courriel a un inconnu serait une faute qu'on ne
    // decouvrirait qu'apres.
    const adresses = plan()
      .enregistrements.map((e) => e.champs['Email'])
      .filter((a): a is string => typeof a === 'string');
    expect(adresses.length).toBeGreaterThan(0);
    for (const a of adresses) expect(a.endsWith(`@${DOMAINE}`)).toBe(true);
    expect(DOMAINE.endsWith('.invalid')).toBe(true);
  });

  it('ecrit des adresses normalisees, comparables aux cles fortes', () => {
    // `normaliserIdentifiant('email_token')` fait `trim` puis minuscules, et les
    // accents ne survivent pas a une adresse : deux graphies donneraient deux
    // jetons, et la jointure serait perdue sans que personne ne le voie.
    expect(adresse('Awa', 'Traoré')).toBe(`awa.traore@${DOMAINE}`);
    expect(adresse('Camille', 'Berthier')).toBe(`camille.berthier@${DOMAINE}`);
  });

  it('plante les canaris HORS de tout perimetre', async () => {
    // Un temoin plante dans un champ que l'une des taches lit legitimement ne
    // temoignerait de rien : il ressortirait dans l'episode a bon droit.
    const perimetres = champsDeTousLesPerimetres();
    for (const champ of Object.keys(CHAMPS_CANARIS)) {
      expect(perimetres.has(champ), `${champ} est dans un perimetre`).toBe(false);
    }
  });

  it('plante exactement les canaris que le registre declare', async () => {
    // Deux listes qui divergent, c'est un temoin plante que personne ne cherche
    // — ou un temoin cherche que personne n'a plante.
    const registre = JSON.parse(await readFile(CANARIS, 'utf8')) as {
      hors_perimetre: { chaines: string[] };
    };
    expect([...plan().canaris].sort()).toEqual([...registre.hors_perimetre.chaines].sort());
  });

  it('ne plante jamais une forme INTERDITE', async () => {
    // Celles-la doivent etre absentes partout. En semer une dans l'org
    // condamnerait le sweep a rouge pour toujours, sans qu'il ait rien trouve.
    const registre = JSON.parse(await readFile(CANARIS, 'utf8')) as {
      interdites: { chaines: string[] };
    };
    const serialise = JSON.stringify(plan());
    for (const c of registre.interdites.chaines) expect(serialise).not.toContain(c);
  });

  it('demande l historique des champs qu il met sous perimetre', () => {
    // Sans historique active, `LeadHistory` rend une liste VIDE — qui ressemble
    // a « rien n'a change » alors qu'elle veut dire « je ne sais pas ». Les deux
    // menent a des conclusions opposees.
    const suivis = new Set(plan().historique_requis.Lead);
    for (const champ of champsDeTousLesPerimetres()) {
      expect(suivis.has(champ), `${champ} n est pas suivi`).toBe(true);
    }
    expect(suivis.size).toBeLessThanOrEqual(20); // la limite de Salesforce
  });

  it('met un texte long sous perimetre, exprès', () => {
    // Deuxieme piege du design §5 : l'historique d'un texte long ne stocke pas
    // ses valeurs. Un jalon qui ne l'exercerait pas prouverait moins qu'il n'en
    // a l'air.
    expect(TACHES['maj-crm-avec-note']?.scope_fields).toContain('Description');
  });

  it('produit exactement le terrain.example.json du depot', async () => {
    // Le fichier committe est le MIROIR du plan, et c'est lui que le validateur
    // Rust relit. Deux sources qui derivent, c'est un exemple qui documente un
    // terrain que le code refuserait.
    const exemple = JSON.parse(await readFile(EXEMPLE, 'utf8')) as unknown;
    expect(plan().terrain).toEqual(exemple);
  });
});
