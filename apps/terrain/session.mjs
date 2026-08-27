/**
 * Ouvrir une session sur l'org de démo, et en tirer un jeton d'API.
 *
 * ## Pourquoi passer par le navigateur
 *
 * La doctrine d'exécution descend l'échelle : API → CLI → MCP → Playwright →
 * humain guidé. Ici, l'API demande un jeton, et le jeton demande une application
 * connectée, qui se crée dans l'interface. On commence donc par le navigateur —
 * une fois — puis tout le reste passe par l'API REST, qui est déterministe là où
 * une interface ne l'est jamais.
 *
 * Le compte est celui de l'agent (D13), ses identifiants sont dans son coffre.
 * Ce n'est pas le compte d'un utilisateur : l'irréductible « secrets » de la
 * doctrine vise les identifiants de l'opérateur, qu'on ne demande jamais et
 * qu'il tape lui-même.
 *
 * ## Le jeton
 *
 * Salesforce accepte l'identifiant de session (`sid`) comme jeton porteur sur
 * son API REST, depuis la même instance. C'est ce qui permet de peupler l'org
 * sans créer d'application connectée au préalable — l'application connectée
 * reste nécessaire pour OAuth PKCE (tâche 2), mais elle n'est pas un préalable
 * au peuplement.
 */
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const CHROME = process.env['NOE_CHROME'] ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';

const attendre = (ms) => new Promise((f) => setTimeout(f, ms));

/**
 * Se connecte et rend `{ instance, jeton, fermer }`.
 *
 * `fermer()` tue le navigateur et efface le profil. À appeler dans un `finally` :
 * un profil de navigateur abandonné contient une session ouverte sur l'org.
 */
export async function ouvrirSession(coffre, { visible = false } = {}) {
  const profil = mkdtempSync(join(tmpdir(), 'noe-terrain-'));
  const port = Number(process.env['NOE_CDP_TERRAIN'] ?? 9340);

  const chrome = spawn(
    CHROME,
    [
      `--user-data-dir=${profil}`,
      '--no-first-run',
      '--no-default-browser-check',
      `--remote-debugging-port=${port}`,
      ...(visible ? [] : ['--headless=new']),
      'about:blank',
    ],
    { stdio: 'ignore' },
  );
  await attendre(2500);

  const nav = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const ctx = nav.contexts()[0];
  const page = ctx.pages()[0] ?? (await ctx.newPage());

  const fermer = async () => {
    try {
      await nav.close();
    } catch {
      /* le navigateur est peut-être déjà parti */
    }
    chrome.kill();
    try {
      rmSync(profil, { recursive: true, force: true });
    } catch {
      /* le dossier temporaire du système s'en occupera */
    }
  };

  try {
    await page.goto(`${coffre.url}/`, { waitUntil: 'domcontentloaded', timeout: 60_000 });

    // Le formulaire de connexion classique. `#username` / `#password` sont
    // stables depuis des années ; s'ils changeaient, l'échec serait franc.
    const champUtilisateur = page.locator('#username');
    if (await champUtilisateur.isVisible().catch(() => false)) {
      await champUtilisateur.fill(coffre.utilisateur);
      await page.locator('#password').fill(coffre.motdepasse);
      await page.locator('#Login').click();
      await page.waitForLoadState('domcontentloaded', { timeout: 60_000 });
      await attendre(4000);
    }

    // Le `sid` n'existe que si la connexion a réussi. Son absence est donc le
    // diagnostic — inutile de deviner en lisant le titre de la page.
    const cookies = await ctx.cookies();
    const sid = cookies.find((c) => c.name === 'sid' && c.domain.includes('salesforce'));
    if (sid === undefined) {
      const url = page.url();
      throw new Error(
        `connexion refusee : aucun cookie de session. URL courante ${url}. ` +
          `Verification en deux etapes ou mot de passe expire ?`,
      );
    }

    // L'instance réelle peut différer de l'URL de connexion (redirection).
    const instance = new URL(page.url()).origin;
    return { instance, jeton: sid.value, page, ctx, fermer };
  } catch (e) {
    await fermer();
    throw e;
  }
}

/**
 * Un appel à l'API REST de Salesforce.
 *
 * Volontairement minimal : pas de client, pas de retry. Le client robuste de la
 * spec 003 (R5) vit dans `@noe/core` et sert la CAPTURE ; ceci n'est qu'un
 * outil de terrain, qui prépare l'org une fois. Les confondre ferait passer la
 * préparation pour de la production.
 */
export async function api(session, chemin, options = {}) {
  const r = await fetch(`${session.instance}${chemin}`, {
    ...options,
    headers: {
      Authorization: `Bearer ${session.jeton}`,
      'Content-Type': 'application/json',
      ...(options.headers ?? {}),
    },
  });
  const texte = await r.text();
  let corps = null;
  try {
    corps = texte.length > 0 ? JSON.parse(texte) : null;
  } catch {
    corps = texte;
  }
  return { ok: r.ok, statut: r.status, corps };
}
