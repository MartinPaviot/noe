/**
 * Les quatre baselines exigées par D21, sur le squelette traversant (D26).
 *
 * Elles tournent sur des **fixtures versionnées**, jamais sur les épisodes du
 * poste : une baseline qui dépendrait de ce que l'opérateur a capturé hier ne
 * resterait pas valide une journée. L'application, elle, lit bien les vraies
 * données — les deux ne servent pas à la même chose.
 *
 * L'état **vide** n'est pas un cas dégradé qu'on tolère : c'est l'écran du
 * premier jour, celui que l'opérateur verra en premier. Il mérite autant de soin
 * que l'état plein, et c'est pour ça qu'il a sa baseline.
 */
import { expect, test } from '@playwright/test';

/** Attend que la vue ait fini de garnir ses frises. */
async function pret(page: import('@playwright/test').Page): Promise<void> {
  await expect(page.locator('#app')).toHaveAttribute('data-pret', 'oui');
}

test('etat nominal — avec des episodes', async ({ page }) => {
  await page.goto('/');
  await pret(page);

  // Les trois grades doivent etre visibles : c'est ce qui fait de cette
  // baseline un controle utile plutot qu'une photo de decor.
  await expect(page.locator('.grade-A')).toHaveCount(1);
  await expect(page.locator('.grade-B')).toHaveCount(1);
  await expect(page.locator('.grade-C')).toHaveCount(1);
  await expect(page.locator('.pt-trou')).toHaveCount(2);

  await expect(page).toHaveScreenshot('nominal.png', { fullPage: true });
});

test('etat vide — le jour 1', async ({ page }) => {
  await page.goto('/?etat=vide');
  await expect(page.locator('.etat-vide')).toBeVisible();

  // L'ecran du premier jour doit dire QUOI FAIRE, pas seulement qu'il n'y a
  // rien. Un ecran vide muet se lit comme une panne.
  await expect(page.locator('.etat-vide')).toContainText('tache active');
  await expect(page.locator('kbd')).toHaveCount(3);

  await expect(page).toHaveScreenshot('vide.png', { fullPage: true });
});

test('etat erreur — le dossier est illisible', async ({ page }) => {
  await page.goto('/?etat=erreur');
  await expect(page.locator('.etat-erreur')).toBeVisible();

  // Une erreur doit dire ce qui reste vrai : les episodes sont sur le disque.
  await expect(page.locator('.etat-erreur')).toContainText('Rien n');
  await expect(page.locator('[role="alert"]')).toBeVisible();

  await expect(page).toHaveScreenshot('erreur.png', { fullPage: true });
});

test('etat chargement', async ({ page }) => {
  await page.goto('/?etat=chargement');
  await expect(page.locator('.etat-chargement')).toBeVisible();
  await expect(page.locator('[aria-busy="true"]')).toBeVisible();

  await expect(page).toHaveScreenshot('chargement.png', { fullPage: true });
});

/**
 * Pas une baseline : un garde-fou.
 *
 * Le squelette affiche des noms de cibles et des raisons de grade — du texte
 * venu de la capture. Si la redaction amont lâchait, c'est ici qu'on le verrait
 * en clair. Le test échoue sur la première PII rendue.
 */
test('aucune PII ne s affiche dans la vue', async ({ page }) => {
  await page.goto('/');
  await pret(page);

  const texte = (await page.locator('#app').innerText()).replace(/\s+/g, ' ');
  const motifs: readonly [string, RegExp][] = [
    ['EMAIL', /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/],
    ['TEL_FR', /(?:\+33[ .-]?|0)[1-9](?:[ .-]?\d{2}){4}/],
    ['IBAN', /\b[A-Z]{2}\d{2}[A-Z0-9]{10,30}\b/],
    ['CARTE', /\b(?:\d{4}[ -]?){3}\d{4}\b/],
  ];
  for (const [nom, motif] of motifs) {
    expect(texte, `${nom} affiche dans la vue`).not.toMatch(motif);
  }
});
