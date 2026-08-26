import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
// @ts-expect-error — module JS sans typage, volontairement (outillage, pas domaine).
import { analyser } from './lint-anti-contenu.mjs';

describe('lint anti-contenu', () => {
  it('refuse une colonne jsonb', () => {
    const v = analyser('create table t (\n  charge jsonb not null\n);');
    expect(v).toHaveLength(1);
    expect(v[0].motif).toContain('jsonb');
  });

  it('refuse une colonne bytea', () => {
    expect(analyser('create table t (\n  piece bytea\n);')).toHaveLength(1);
  });

  it('refuse une colonne textuelle au nom evocateur', () => {
    const v = analyser('create table t (\n  corps_message text\n);');
    expect(v).toHaveLength(1);
    expect(v[0].colonne).toBe('corps_message');
  });

  it('refuse un ajout de colonne hors create table', () => {
    expect(analyser('alter table t add column contenu text;')).toHaveLength(1);
  });

  it('accepte les colonnes legitimes du socle', () => {
    const sql = `create table t (
  id uuid primary key,
  statut text not null,
  valeur bigint not null default 0,
  emis_le timestamptz not null
);`;
    expect(analyser(sql)).toHaveLength(0);
  });

  it('respecte une autorisation explicite', () => {
    const sql = `create table t (
  -- noe:contenu-autorise empreinte opaque, jamais le contenu lui-meme
  charge jsonb
);`;
    expect(analyser(sql)).toHaveLength(0);
  });

  it('la migration reelle du socle passe le lint', () => {
    const sql = readFileSync('supabase/migrations/20260826110000_socle_licences.sql', 'utf8');
    expect(analyser(sql, 'socle')).toEqual([]);
  });
});
