/**
 * @noe/harness — rejeu et jugement mecanique.
 * Vide en session 0 : le juge est la feature F04, le rejeu la F03.
 */

export const AIDE = `noe — harness de rejeu et de jugement

Usage
  noe <commande> [options]

Commandes (aucune n'est implementee en session 0)
  replay <corpus>     Rejoue un corpus d'episodes de maniere deterministe.
  judge <run>         Applique le juge mecanique a un run de rejeu.
  fixtures            Liste les corpus dores disponibles localement.

Regle
  Seul le juge mecanique promeut. Aucune sortie de ce CLI ne quitte le poste.
`;
