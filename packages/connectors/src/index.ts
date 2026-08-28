/**
 * @noe/connectors — les systemes de verite que Noe sait brancher.
 *
 * **Les noms viennent d'ici et se verifient contre le terrain.** Ils vivaient en
 * trois exemplaires : ce fichier, les adaptateurs Rust, et `terrain.json`. Les
 * deux premiers avaient deja diverge — ce paquet annoncait `google-workspace`
 * la ou l'adaptateur s'appelle `gmail` — sans que rien ne le dise, parce que le
 * seul banc du paquet comptait la longueur du tableau.
 *
 * L'implementation, elle, vit en Rust : la resolution et les lectures se font
 * pendant l'episode ouvert, et le harness ne voit les episodes qu'apres coup.
 */

/** Les connecteurs que le capteur sait parler. */
export const SYSTEMES_ENVISAGES = ['salesforce', 'gmail'] as const;

export type SystemeDeVerite = (typeof SYSTEMES_ENVISAGES)[number];
