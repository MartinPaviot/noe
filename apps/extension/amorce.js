/**
 * L'amorce : trois lignes de script classique, pour charger un vrai module.
 *
 * MV3 déclare les scripts de contenu en **script classique** — `import` y est une
 * erreur de syntaxe. Sans cette amorce, tout le capteur devrait vivre dans un
 * seul fichier sans `export`, donc sans test unitaire possible : on vérifierait
 * l'ancrage en le relisant, ce qui n'est pas le vérifier.
 *
 * Le prix est connu et assumé : l'import dynamique est asynchrone, donc le
 * branchement arrive quelques millisecondes après `document_start` au lieu de
 * pile dessus. Ce qui compte est que le balayage des racines shadow ait lieu et
 * se rejoue sur mutation ; il se rejoue.
 */
import(chrome.runtime.getURL('contenu.js')).catch((e) => {
  // Un capteur qui ne se charge pas doit le DIRE. Silencieux, il ferait croire à
  // une page sans activité, ce qui est le pire des trois états possibles.
  console.error('[noe] capteur non charge :', e);
});
