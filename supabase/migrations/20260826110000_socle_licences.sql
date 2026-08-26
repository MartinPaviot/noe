-- Socle commercial de Noe : licences, compteurs, consentement télémétrie.
--
-- INVARIANT I — aucun contenu utilisateur ne quitte le poste.
-- Ce schéma ne porte QUE des identifiants, des états et des nombres.
-- Aucune colonne ne peut accueillir un épisode, un courriel, un enregistrement
-- CRM ou un fragment de travail réel. Le lint anti-contenu (`pnpm lint:sql`)
-- le vérifie mécaniquement à chaque commit.

-- ---------------------------------------------------------------------------
-- licences
-- ---------------------------------------------------------------------------
create table public.licences (
  id                     uuid primary key default gen_random_uuid(),

  -- La clé émise, signée ed25519. Vérifiée HORS LIGNE par l'app : le serveur
  -- n'est jamais interrogé pour valider une licence.
  cle_licence            text        not null unique,

  -- Rattachement Stripe. Identifiants opaques, aucune donnée de paiement.
  stripe_customer_id     text        not null,
  stripe_subscription_id text,

  -- Adresse de facturation : nécessaire pour délivrer la clé. C'est une donnée
  -- personnelle, pas du contenu de travail — la distinction est volontaire.
  email_facturation      text        not null,

  statut                 text        not null
    check (statut in ('en_essai', 'active', 'expiree', 'revoquee')),

  emis_le                timestamptz not null default now(),
  expire_le              timestamptz,
  revoque_le             timestamptz,
  motif_revocation       text
    check (motif_revocation in ('remboursement', 'fraude', 'demande_client', 'migration')),

  constraint revocation_coherente
    check ((statut = 'revoquee') = (revoque_le is not null))
);

comment on table public.licences is
  'Licences emises. Aucun contenu utilisateur : identifiants, etats et dates.';

create index licences_stripe_customer_idx on public.licences (stripe_customer_id);
create index licences_statut_idx          on public.licences (statut);

-- ---------------------------------------------------------------------------
-- compteurs — des nombres, exclusivement
-- ---------------------------------------------------------------------------
create table public.compteurs (
  id         bigserial primary key,
  licence_id uuid   not null references public.licences (id) on delete cascade,

  -- Premier jour du mois couvert.
  periode    date   not null,

  -- Nom du compteur, pris dans une liste fermee. Une liste fermee interdit
  -- qu'un compteur devienne un fourre-tout a contenu.
  nom        text   not null
    check (nom in (
      'episodes_captures',
      'trous_de_capture',
      'brouillons_produits',
      'brouillons_valides',
      'rejeux_executes'
    )),

  valeur     bigint not null default 0 check (valeur >= 0),
  maj_le     timestamptz not null default now(),

  unique (licence_id, periode, nom)
);

comment on table public.compteurs is
  'Compteurs mensuels. Uniquement des entiers, jamais de contenu.';

-- ---------------------------------------------------------------------------
-- telemetrie_optin — consentement explicite, revocable
-- ---------------------------------------------------------------------------
create table public.telemetrie_optin (
  licence_id  uuid primary key references public.licences (id) on delete cascade,
  consenti    boolean     not null default false,
  consenti_le timestamptz,
  revoque_le  timestamptz,

  constraint consentement_coherent
    check ((consenti = true) = (consenti_le is not null and revoque_le is null))
);

comment on table public.telemetrie_optin is
  'Consentement telemetrie. Par defaut FAUX : jamais d''opt-out, toujours opt-in.';

-- ---------------------------------------------------------------------------
-- RLS — activee sur les trois tables, AUCUNE politique
-- ---------------------------------------------------------------------------
-- RLS active sans politique = tout est refuse pour anon et authenticated.
-- Seul service_role (webhook Stripe, cote serveur) contourne la RLS.
-- On ouvrira au cas par cas, jamais par defaut.

alter table public.licences         enable row level security;
alter table public.compteurs        enable row level security;
alter table public.telemetrie_optin enable row level security;

alter table public.licences         force row level security;
alter table public.compteurs        force row level security;
alter table public.telemetrie_optin force row level security;

-- Revocation explicite : meme sans RLS, aucun droit de table pour les roles
-- publics. Ceinture et bretelles, volontairement.
revoke all on public.licences         from anon, authenticated;
revoke all on public.compteurs        from anon, authenticated;
revoke all on public.telemetrie_optin from anon, authenticated;
