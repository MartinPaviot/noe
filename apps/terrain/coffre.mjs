/**
 * Lit un secret du coffre DPAPI — **en mémoire, jamais sur disque**.
 *
 * Le coffre est chiffré pour le compte Windows courant. Le déchiffrer demande
 * PowerShell ; ce module l'appelle, récupère le clair par un tuyau, et le rend à
 * l'appelant. À aucun moment la valeur ne touche un fichier.
 *
 * Cette précaution n'est pas décorative. Un secret écrit dans un fichier
 * temporaire y reste après le processus, se retrouve dans les sauvegardes, et
 * échappe à toute rotation. Un secret passé en argument de commande apparaît dans
 * la liste des processus, où n'importe quel programme du poste peut le lire.
 *
 * La règle 5 du projet dit « jamais un secret dans un fichier suivi ». Celle-ci
 * est plus stricte, et c'est volontaire : **jamais un secret dans un fichier**.
 */
import { execFileSync } from 'node:child_process';

/**
 * Le coffre, déjà ouvert par l'appelant.
 *
 * Le bac à sable de l'environnement d'exécution refuse à Node de lancer
 * PowerShell — ce qui est une bonne chose : un processus qui peut déchiffrer un
 * coffre peut déchiffrer tous les coffres. On accepte donc que l'ouverture se
 * fasse **en amont**, et que le clair arrive par l'environnement.
 *
 * L'environnement plutôt qu'un fichier : un fichier temporaire survit au
 * processus, entre dans les sauvegardes, et échappe à toute rotation. Plutôt
 * qu'un argument de commande : `argv` apparaît dans la liste des processus, où
 * n'importe quel programme du poste peut le lire. L'environnement d'un processus
 * n'est lisible que par le même compte — la même frontière que le coffre.
 */
function depuisEnvironnement() {
  const brut = process.env['NOE_COFFRE'];
  if (brut === undefined || brut.length === 0) return null;
  return JSON.parse(brut);
}

/**
 * Ouvre un coffre et rend son contenu déchiffré.
 *
 * @param {string} chemin Le fichier `.dpapi`.
 * @returns {Record<string, string>}
 */
export function ouvrirCoffre(chemin) {
  const dejaOuvert = depuisEnvironnement();
  if (dejaOuvert !== null) return dejaOuvert;

  const ps = `
    Add-Type -AssemblyName System.Security
    $brut = [System.IO.File]::ReadAllBytes('${chemin.replace(/'/g, "''")}')
    $clair = [System.Security.Cryptography.ProtectedData]::Unprotect(
      $brut, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
    [System.Text.Encoding]::UTF8.GetString($clair)
  `;
  const brut = execFileSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', ps], {
    encoding: 'utf8',
    stdio: 'pipe',
    // Le secret transite par ce tuyau et rien d'autre. `maxBuffer` généreux :
    // un coffre tronqué donnerait un JSON invalide, donc une erreur obscure.
    maxBuffer: 1024 * 1024,
  });
  return JSON.parse(brut.trim());
}

/**
 * Masque une valeur pour l'affichage.
 *
 * Rien de ce qui vient du coffre ne s'imprime en clair, pas même « pour
 * déboguer » — c'est exactement là que les secrets fuient, dans une sortie
 * qu'on croyait éphémère et qui finit dans un fichier de journal.
 */
export const masquer = (v) => `(${String(v).length} caracteres)`;
