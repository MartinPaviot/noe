#!/usr/bin/env node
import { Command } from 'commander';
import { type Policy, politiqueNulle, politiqueParfaite } from './policy.js';
import { chargerCorpus, codeSortie, EXIT_ERREUR, rejouer } from './replay.js';
import { rapportJson, rapportTexte } from './report.js';

/**
 * CLI du harness. Deux commandes, aucun réseau.
 *
 * Codes de sortie (R3.5) : 0 tous les verdicts conformes · 1 au moins un écart ·
 * 2 erreur d'exécution. Utilisable tel quel en CI.
 */

async function construirePolitique(nom: string, dossier: string): Promise<Policy> {
  if (nom === 'nulle') return politiqueNulle;
  if (nom === 'parfaite') {
    // La politique parfaite reçoit le corpus à la construction : elle ne lit
    // jamais l'état d'après depuis le contexte de rejeu.
    const { episodes } = await chargerCorpus(dossier);
    return politiqueParfaite(episodes);
  }
  throw new Error(`politique inconnue : « ${nom} » (attendu : parfaite | nulle)`);
}

const programme = new Command();

programme
  .name('noe')
  .description('Harness de rejeu et de jugement mecanique. Aucun appel reseau.')
  .version('1.0.0');

programme
  .command('replay')
  .description('Rejoue un corpus d episodes en mode fixtures, hors ligne.')
  .argument('<dossier>', 'dossier contenant les episodes .json')
  .option('--json', 'sortie JSON stable, pour la machine')
  .option('--policy <nom>', 'politique a rejouer : parfaite | nulle', 'parfaite')
  .action(async (dossier: string, opts: { json?: boolean; policy: string }) => {
    try {
      const politique = await construirePolitique(opts.policy, dossier);
      const rapport = await rejouer(dossier, politique);
      process.stdout.write(
        opts.json === true ? `${rapportJson(rapport)}\n` : rapportTexte(rapport),
      );
      process.exitCode = codeSortie(rapport);
    } catch (e) {
      process.stderr.write(`noe replay : ${e instanceof Error ? e.message : String(e)}\n`);
      process.exitCode = EXIT_ERREUR;
    }
  });

programme
  .command('judge')
  .description('Rejoue puis ne rend que le bilan agrege.')
  .argument('<dossier>', 'dossier contenant les episodes .json')
  .option('--summary', 'bilan agrege (comportement par defaut)')
  .option('--json', 'sortie JSON stable')
  .option('--policy <nom>', 'politique a rejouer : parfaite | nulle', 'parfaite')
  .action(async (dossier: string, opts: { json?: boolean; policy: string }) => {
    try {
      const politique = await construirePolitique(opts.policy, dossier);
      const rapport = await rejouer(dossier, politique);

      if (opts.json === true) {
        process.stdout.write(`${rapportJson({ ...rapport, episodes: [] })}\n`);
      } else {
        const a = rapport.agregat;
        process.stdout.write(
          [
            `corpus    ${rapport.corpus}`,
            `politique ${rapport.politique}`,
            '',
            `episodes  ${a.n_total}   comptes ${a.n_comptes}   exclus ${a.n_exclus}`,
            `accord    ${a.n_accord}/${a.n_comptes}  (${a.taux_accord} %)`,
            `classes   accord ${a.par_classe.accord} · desaccord ${a.par_classe.desaccord} · manque ${a.par_classe.manque} · excedent ${a.par_classe.excedent} · hors_perimetre ${a.par_classe.hors_perimetre}`,
            ...(a.champs_en_echec.length > 0
              ? ['', 'champs en echec', ...a.champs_en_echec.map((c) => `    ${c.n}  ${c.champ}`)]
              : []),
            '',
          ].join('\n'),
        );
      }
      process.exitCode = codeSortie(rapport);
    } catch (e) {
      process.stderr.write(`noe judge : ${e instanceof Error ? e.message : String(e)}\n`);
      process.exitCode = EXIT_ERREUR;
    }
  });

programme.parseAsync(process.argv).catch((e: unknown) => {
  process.stderr.write(`noe : ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = EXIT_ERREUR;
});
