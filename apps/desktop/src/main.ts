/**
 * Coquille desktop de Noe.
 * Session 0 : elle compile, elle s'ouvre, elle ne fait rien d'autre.
 * Aucune capture, aucun reseau, aucun acces au systeme de fichiers.
 */

const RACINE_ID = 'app';

function monter(): void {
  const racine = document.getElementById(RACINE_ID);
  if (racine === null) {
    throw new Error(`Element #${RACINE_ID} introuvable dans index.html`);
  }
  racine.textContent = 'Noe — coquille vide (session 0).';
}

monter();
