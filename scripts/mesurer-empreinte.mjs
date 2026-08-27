/**
 * Le script de mesure de R7.1.
 *
 * L'exigence dit « mesurés par le script de mesure fourni » : elle exige donc
 * qu'un script existe, qu'il soit rejouable, et que le chiffre qu'il rend soit
 * le même pour l'opérateur que pour moi. Le voici.
 *
 * ## Ce qu'il mesure
 *
 * Le **processus de Noe**, pas la machine. Un opérateur ne désinstalle pas Noe
 * parce que Windows indexe son disque ; il le désinstalle parce que Noe chauffe.
 * Mesurer la charge globale ferait échouer la mesure pour des raisons qui ne la
 * concernent pas — et, pire, laisserait passer une fuite de Noe sur une machine
 * par ailleurs oisive.
 *
 * Le CPU est rapporté en **pourcentage d'un cœur**, comme dans l'application :
 * `Get-Counter` rend un pourcentage de la machine entière sur les systèmes
 * multi-cœurs, ce qui diviserait le chiffre par le nombre de cœurs et rendrait
 * n'importe quel budget facile à tenir. On calcule donc le delta de temps CPU
 * sur le delta d'horloge, exactement comme `empreinte.rs`.
 *
 * ## Ce qu'il ne mesure pas
 *
 * Il ne lance rien et ne pilote rien. Il observe un processus déjà là, pendant
 * que quelqu'un — l'opérateur ou un banc — fait le travail. Un script qui
 * fabriquerait sa propre charge mesurerait sa propre charge.
 *
 * Usage :
 *   node scripts/mesurer-empreinte.mjs [--processus noe-desktop] [--fenetres 5]
 */
import { execFileSync } from 'node:child_process';

const arg = (nom, defaut) => {
  const i = process.argv.indexOf(`--${nom}`);
  return i < 0 ? defaut : process.argv[i + 1];
};

const PROCESSUS = arg('processus', 'noe-desktop');
const FENETRES = Number(arg('fenetres', '5'));
/** R7.1 : la fenêtre est de 30 s, et ce n'est pas un réglage. */
const FENETRE_MS = 30_000;
const BUDGET_CPU_PCT = 5;
const BUDGET_RAM_MO = 200;

if (!Number.isInteger(FENETRES) || FENETRES < 1) {
  console.error(`--fenetres doit etre un entier positif, recu ${arg('fenetres', '5')}`);
  process.exit(2);
}

/**
 * Le temps CPU total et la mémoire de travail, par PowerShell.
 *
 * `TotalProcessorTime` compte noyau + utilisateur, comme `GetProcessTimes` côté
 * application : les deux mesures doivent être comparables, sinon le script
 * « fourni » de l'exigence dirait autre chose que le produit.
 */
function lire() {
  const ps = `
    $p = Get-Process -Name '${PROCESSUS}' -ErrorAction SilentlyContinue
    if ($null -eq $p) { Write-Output 'ABSENT'; exit 0 }
    # .TotalMilliseconds et pas TotalProcessorTime : Measure-Object refuse un
    # TimeSpan, rend une somme VIDE, et [double]'' vaut 0. La premiere version
    # annoncait 0,00 % a chaque fenetre — un budget tenu en ne mesurant rien.
    $cpu = ($p | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } |
            Measure-Object -Sum).Sum
    $ws  = ($p | Measure-Object -Property WorkingSet64 -Sum).Sum
    # Des ENTIERS, jamais des flottants : la machine est en francais, et « -f »
    # ecrit alors « 1234,5 ». Number(1234 virgule 5) vaut NaN, et la premiere version
    # affichait « NaN % CPU » apres avoir deja affiche « 0,00 % ». Deux mesures
    # fausses de suite, chacune plausible a l'oeil.
    Write-Output ("{0};{1};{2}" -f $p.Count, [long]$cpu, [long]$ws)
  `;
  const brut = execFileSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', ps], {
    encoding: 'utf8',
  }).trim();
  if (brut === 'ABSENT') return null;
  const [n, cpu, ws] = brut.split(';');
  return { instances: Number(n), cpu_ms: Number(cpu), ram_octets: Number(ws) };
}

const dormir = (ms) => new Promise((f) => setTimeout(f, ms));

const premiere = lire();
if (premiere === null) {
  console.error(
    `processus « ${PROCESSUS} » introuvable — lancez l application, ouvrez un episode, puis relancez.`,
  );
  process.exit(1);
}
console.error(
  `mesure de « ${PROCESSUS} » (${premiere.instances} instance(s)) — ${FENETRES} fenetres de 30 s`,
);

let precedent = premiere;
let precedentInstant = Date.now();
const mesures = [];

for (let f = 1; f <= FENETRES; f += 1) {
  await dormir(FENETRE_MS);
  const courant = lire();
  const maintenant = Date.now();
  if (courant === null) {
    console.error('le processus a disparu en cours de mesure');
    process.exit(1);
  }
  const ecoule = maintenant - precedentInstant;
  // TotalProcessorTime est en millisecondes de CPU. Rapporté au temps écoulé,
  // il donne le pourcentage d'UN cœur.
  const cpu_pct = (100 * (courant.cpu_ms - precedent.cpu_ms)) / ecoule;
  const ram_mo = courant.ram_octets / (1024 * 1024);
  mesures.push({ fenetre: f, cpu_pct, ram_mo });
  console.error(
    `  fenetre ${f}/${FENETRES} : ${cpu_pct.toFixed(2)} % CPU, ${ram_mo.toFixed(0)} Mo`,
  );
  precedent = courant;
  precedentInstant = maintenant;
}

/** Le p95 sur si peu de fenêtres, c'est le maximum. On le dit plutôt que de le maquiller. */
const cpuMax = Math.max(...mesures.map((m) => m.cpu_pct));
const ramMax = Math.max(...mesures.map((m) => m.ram_mo));

/**
 * Une mesure qui rend exactement zéro partout n'est pas une bonne nouvelle.
 *
 * C'est ce qui s'est produit à la première exécution : `Measure-Object` refuse
 * un `TimeSpan`, rendait une somme vide, et `[double]''` vaut 0. Le script
 * annonçait « DANS LE BUDGET » à chaque fenêtre en n'ayant rien mesuré. Un vert
 * qui veut dire « je n'ai rien vu » est pire qu'un rouge, et la règle 2 le dit :
 * seul un verdict reproductible promeut.
 */
const illisible = mesures.some((m) => !Number.isFinite(m.cpu_pct) || !Number.isFinite(m.ram_mo));
const aucuneMesure = mesures.length > 0 && mesures.every((m) => m.cpu_pct === 0);
if (illisible || aucuneMesure) {
  console.error(
    illisible
      ? 'REFUS : une fenetre rend une valeur non numerique. La mesure est cassee.'
      : 'REFUS : toutes les fenetres rendent exactement 0 % de CPU. Un processus vivant ' +
          'en consomme toujours un peu. La mesure est cassee, pas le budget tenu.',
  );
  process.exit(3);
}

const tient = cpuMax <= BUDGET_CPU_PCT && ramMax <= BUDGET_RAM_MO;

console.log(
  JSON.stringify(
    {
      processus: PROCESSUS,
      instances: premiere.instances,
      fenetres: mesures,
      cpu_max_pct: Number(cpuMax.toFixed(2)),
      ram_max_mo: Number(ramMax.toFixed(0)),
      budget: { cpu_pct: BUDGET_CPU_PCT, ram_mo: BUDGET_RAM_MO },
      // Avec cinq fenetres, le « p95 » est le maximum. Le nommer autrement
      // laisserait croire a une statistique qu'on n'a pas les moyens de faire.
      note: 'cpu_max_pct est le maximum des fenetres, pas un p95 : cinq points ne font pas une distribution',
      verdict: tient ? 'DANS LE BUDGET' : 'HORS BUDGET',
    },
    null,
    2,
  ),
);
process.exit(tient ? 0 : 1);
