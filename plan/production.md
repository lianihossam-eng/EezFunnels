# Eez Funnel — Plan final de production

**Version** : 1.0
**Date** : 2026-10-13
**Nom commercial** : Eez Funnel
**Repo** : `github.com/eezfunnel/eezfunnel` (public)
**Licence** : MIT
**Type** : SaaS self-hosted single-user
**Cible v0.1.0** : visiter l'URL → dashboard direct → créer/éditer/publier un funnel

---

## 1. Vision

Un funnel builder que tu déploies sur ton propre compte Cloudflare. Tu ouvres l'URL, tu tombes **directement sur le dashboard**, comme si tu étais déjà connecté à un compte client. Tu crées un funnel, tu l'édites visuellement, tu le publies sur `/f/:slug`. Aucune inscription, aucun compte SaaS, aucun abonnement mensuel.

**Single-user by design.** v0.1.0 assume que tu es le seul utilisateur. Auth, landing page, et sous-domaines custom : v0.2+.

---

## 2. Scope v0.1.0

| ID | Fonctionnalité | Sortie observable |
|:---|:---|:---|
| F-001 | Worker health endpoint | `curl /health` → `{"status":"ok"}` |
| F-002 | D1 + R2 + KV bindings | Bindings Cloudflare opérationnels |
| F-003 | Schéma D1 + 5 thèmes seed | Tables créées, thèmes insérés |
| F-004 | Modèle funnel (revisions, render_manifest) | Types Rust |
| F-005 | Moteur de rendu sandboxé | HTML sans script injectable |
| F-006 | 5 sections | hero, features, testimonials, cta, footer |
| F-007 | 5 thèmes | minimal, bold, elegant, playful, corporate |
| F-008 | Dashboard shell | Dioxus WASM servi à `/` |
| F-009 | Liste funnels + création | CRUD via API |
| F-010 | Éditeur visuel | Drag-and-drop + clavier |
| F-011 | Inspecteur de props | Formulaire dynamique |
| F-012 | Undo/redo | Ctrl+Z / Ctrl+Y |
| F-013 | Aperçu responsive | 375, 768, 1280 px |
| F-014 | Publication `/f/:slug` | Funnel public |
| F-015 | Command registry | Dispatcher unique |
| F-016 | CLI `eez` | `eez funnels list`, `eez deploy` |
| F-017 | MCP server `/mcp` | `execute_command` via Claude Desktop |
| F-018 | Documentation | README + docs/ + exemples |
| F-019 | Tests E2E + qualité | Playwright + axe-core + fuzz |
| F-020 | Release v0.1.0 | Tag, binaires, notes |

**Hors scope** : auth, landing, sous-domaines custom, CRM, analytics, paiements, emails, IA, multi-tenant.

---

## 3. Architecture

```
┌──────────────────────────────────────────────────────┐
│  Cloudflare Worker (Rust, Axum)                      │
│                                                       │
│  GET /                    → Dashboard (Dioxus WASM)  │
│  GET /f/:slug             → Funnel publié (SSR)      │
│  /api/*                   → REST API                 │
│  /mcp                     → MCP server (HTTP)        │
│  /assets/*                → Assets R2                │
│                                                       │
│  Single-user implicite (pas d'auth)                  │
└────────┬─────────────────────────────────────────────┘
         │
    ┌────┴────┬──────────┐
    ▼         ▼          ▼
┌───────┐ ┌──────┐ ┌────────┐
│  D1   │ │  R2  │ │   KV   │
│ SQLite│ │assets│ │ cache  │
└───────┘ └──────┘ └────────┘
```

**8 unités Cargo** :
- `crates/domain` — types, contrats, invariants
- `crates/render` — rendu HTML sandboxé
- `crates/storage` — bindings D1/R2/KV
- `crates/commands` — dispatcher, policies
- `apps/api` — Worker HTTP
- `apps/dashboard` — Dioxus WASM
- `apps/cli` — binaire `eez`
- `xtask` — automation

**Layering** (enforced par `cargo xtask layers`) :
```
L0: domain
L1: render
L2: storage
L3: commands
L4: api, dashboard, cli
```

---

## 4. Stack technique

| Composant | Technologie | Version |
|:---|:---|:---|
| Langage | Rust | 1.85.0 |
| Web framework | Axum | 0.8 |
| Cloudflare adapter | axum-cloudflare-adapter | 0.14 |
| Workers bindings | workers-rs | 0.8 |
| Frontend | Dioxus WASM | 0.7 |
| Base de données | D1 (SQLite) | Cloudflare |
| Stockage objets | R2 | Cloudflare |
| Cache | KV | Cloudflare |
| MCP | rmcp | 1.4+ |
| CLI framework | clap | 4 |
| Sérialisation | serde / serde_json | 1 |

---

## 5. Méthodologie

**Skills d'exécution** : `cursor/plugins/pstack`
- 23 playbooks via `/poteto-mode`
- `/architect`, `/tdd`, `/no-comments`, `/interrogate`, `/opening-a-pr`
- 24 principes (type-system-discipline, boundary-discipline, prove-it-works)

**Skills de delivery** : `mattpocock/skills`
- `/grill-with-docs`, `/to-spec`, `/to-tickets`, `/implement`, `/code-review`, `/retro`
- `/diagnosing-bugs`, `/improve-codebase-architecture`

**Règle non-négociable — Never Crash** :
- Zéro `unwrap()`, `expect()`, `panic!`, `todo!`, `unimplemented!` sur chemins non-test
- Zéro `unsafe`
- Inputs hostiles : `get()` au lieu de `[i]`, arithmétique checked, récursion bornée
- Chaque fix de crash → test de régression synthétique
- Fuzzing 10 000 cas par frontière

**Règle Clean-room** :
- Aucune lecture de code Shopify, ClickFunnels, Systeme.io, GoHighLevel, Kartra
- Aucun asset copié

**Discipline commits** :
- Conventionnels (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, `refactor:`)
- Un task-ID par commit (`Task-ID: PR-XXX`)
- `cargo xtask check` vert avant chaque commit
- `/no-comments` + `/interrogate` avant chaque commit

---

## 6. Plan d'exécution — PR par PR

### Phase 0 — Bootstrap (semaine 1)

#### PR-000 — Repo + workspace + fondations

**Branche** : `chore/000_bootstrap`

| Commit | Contenu |
|:---|:---|
| `chore: init cargo workspace (8 units)` | 4 crates + 3 apps + xtask |
| `chore: pin rust-toolchain (1.85.0)` | — |
| `docs: README.md` | Quickstart, features, architecture |
| `docs: LICENSE (MIT)` | — |
| `docs: CONTRIBUTING.md` | Guide de contribution |
| `docs: CODE_OF_CONDUCT.md` | Contributor Covenant v2.1 |
| `docs: SECURITY.md` | Politique de divulgation |
| `docs: AGENTS.md` | Never Crash, Clean-room, command-based, pstack |
| `docs: GLOSSARY.md` | Vocabulaire |
| `docs: plan/STATUS.md` | Versionné |
| `docs: plan/architecture.md` | Versionné |
| `docs: plan/coverage.md` | Registre F-001 → F-020 |
| `docs: plan/naming.md` | Analyse "Eez Funnel" vs "EZ Funnels" |
| `docs: plan/contracts/C-01_funnel.md` | Contrat funnel |
| `docs: plan/contracts/C-01_config.md` | Valeurs gelées |
| `docs: plan/contracts/C-03_security.md` | Invariants sécurité |
| `docs: plan/contracts/C-04_errors.md` | Taxonomie erreurs |
| `docs: plan/decisions/D1-D5.md` | Décisions actées |
| `ci: .github/workflows/ci.yml` | fmt + clippy + test + wasm + audit |
| `ci: .github/workflows/release.yml` | Binaires linux/macos/windows |
| `chore: .gitignore, .editorconfig, .rustfmt.toml` | — |

**Critère** : `cargo xtask check` vert. Repo public. CI active.

---

#### PR-001 — xtask à deux niveaux

**Branche** : `feat/001_xtask`

| Commit | Contenu |
|:---|:---|
| `feat(xtask): check (fmt + clippy + test + layers)` | À chaque commit |
| `feat(xtask): ci (check + wasm + audit)` | Avant merge |
| `feat(xtask): layers (domain ≠ render ≠ storage ≠ api)` | Vérifie le layering |
| `feat(xtask): deploy (wrangler wrapper)` | Déploiement |
| `test(xtask): check passes on clean workspace` | — |

---

### Phase 1 — Runtime Cloudflare (semaine 2)

#### PR-002 — Worker + D1 + R2 + KV

**Branche** : `feat/002_worker`

| Commit | Contenu |
|:---|:---|
| `feat(api): worker with /health` | Axum via adapter |
| `feat(api): D1 binding + migrations runner` | `wrangler d1 migrations apply` |
| `feat(api): R2 binding for assets` | — |
| `feat(api): KV binding for cache` | — |
| `feat(api): host validation (allowlist)` | — |
| `feat(api): structured logging (correlation_id)` | C-04 |
| `feat(api): error taxonomy (C-04)` | — |
| `chore: wrangler.toml (dev + production)` | Deux envs |
| `test(api): health + db + r2 + kv round-trip` | — |

**Critère** : `curl /health` → 200. `wrangler d1 migrations apply` OK.

---

#### PR-003 — Schéma D1 + seed thèmes

**Branche** : `feat/003_schema`

| Commit | Contenu |
|:---|:---|
| `feat(storage): migrations (funnels, revisions, themes, assets, audit_logs)` | 5 tables |
| `feat(storage): seed 5 thèmes prédéfinis` | minimal, bold, elegant, playful, corporate |
| `feat(storage): repository (CRUD funnels, revisions, themes)` | — |
| `test(storage): round-trip funnel + revision` | — |
| `test(storage): publication atomique` | — |
| `test(storage): schema_version refusals` | Négatifs |

---

### Phase 2 — Modèle et rendu (semaine 3)

#### PR-004 — Contrat funnel + domaine

**Branche** : `feat/004_domain`

| Commit | Contenu |
|:---|:---|
| `feat(domain): funnel, revision, section, render_manifest` | Modèle Rust |
| `feat(domain): schema_version validation` | C-01 |
| `feat(domain): publication (draft → published)` | Atomique |
| `feat(domain): compatibility registry` | — |
| `test(domain): snapshot determinism` | — |
| `test(domain): concurrent publication` | — |

---

#### PR-005 — Moteur de rendu

**Branche** : `feat/005_render`

| Commit | Contenu |
|:---|:---|
| `feat(render): HTML generator (sandboxed)` | Pas de script injectable |
| `feat(render): theme token injection (CSS custom properties)` | — |
| `feat(render): section dispatch by type` | — |
| `test(render): XSS fuzz (10 000 entrées)` | Aucun script injecté |
| `test(render): determinism with render_manifest` | — |

---

#### PR-006 — 5 sections

**Branche** : `feat/006_sections`

| Commit | Contenu |
|:---|:---|
| `feat(render): hero (title, subtitle, cta, image)` | — |
| `feat(render): features (grid)` | — |
| `feat(render): testimonials (list)` | — |
| `feat(render): cta (button)` | — |
| `feat(render): footer (links, copyright)` | — |
| `test(render): snapshot per section` | — |

---

#### PR-007 — 5 thèmes

**Branche** : `feat/007_themes`

| Commit | Contenu |
|:---|:---|
| `feat(domain): 5 preset themes with tokens` | JSON |
| `feat(render): theme token application` | — |
| `feat(commands): theme.set command` | — |
| `test(themes): visual regression` | — |

---

### Phase 3 — Dashboard (semaines 4-6)

#### PR-008 — Dashboard shell + routing

**Branche** : `feat/008_dashboard`

| Commit | Contenu |
|:---|:---|
| `feat(dashboard): Dioxus WASM app` | — |
| `feat(dashboard): served by worker at /` | assets binding |
| `feat(dashboard): routing (/, /funnels, /funnels/:id)` | — |
| `feat(dashboard): sidebar + header + design tokens` | — |
| `test(dashboard): routing + SSR first paint` | — |

**Critère** : ouvrir `/` → dashboard visible.

---

#### PR-009 — Liste funnels + création

**Branche** : `feat/009_funnel_list`

| Commit | Contenu |
|:---|:---|
| `feat(api): GET /api/funnels, POST /api/funnels` | — |
| `feat(dashboard): funnel list page` | — |
| `feat(dashboard): create funnel modal (5 blueprints)` | tripwire, VSL, webinar, launch, lead-gen |
| `feat(api): DELETE /api/funnels/:id` | — |
| `test(api): CRUD funnels` | — |

---

#### PR-010 — Éditeur visuel

**Branche** : `feat/010_editor`

| Commit | Contenu |
|:---|:---|
| `feat(editor): canvas with iframe-sandboxed preview` | — |
| `feat(editor): section tree` | — |
| `feat(editor): drag-and-drop (dioxus-nox-dnd)` | — |
| `feat(editor): keyboard navigation (Tab, Espace, Flèches)` | — |
| `feat(api): PUT /api/funnels/:id/revisions` | Save |
| `test(editor): DnD (souris + clavier)` | — |
| `test(editor): axe-core 0 critique` | — |

---

#### PR-011 — Inspecteur

**Branche** : `feat/011_inspector`

| Commit | Contenu |
|:---|:---|
| `feat(editor): settings inspector` | — |
| `feat(editor): dynamic form from settings_schema` | text, color, select, toggle, image |
| `feat(editor): live preview update` | — |
| `test(editor): inspector tests` | — |

---

#### PR-012 — Undo/redo

**Branche** : `feat/012_history`

| Commit | Contenu |
|:---|:---|
| `feat(editor): undo/redo stack (JSON diffs)` | — |
| `feat(editor): keyboard shortcuts (Ctrl+Z, Ctrl+Y)` | — |
| `test(editor): 100 modifs, undo 50, redo 50` | — |

---

#### PR-013 — Aperçu responsive

**Branche** : `feat/013_responsive`

| Commit | Contenu |
|:---|:---|
| `feat(editor): viewport switcher (375, 768, 1280 px)` | — |
| `feat(editor): iframe resize` | — |
| `test(editor): responsive snapshots` | — |

---

### Phase 4 — Publication (semaine 7)

#### PR-014 — Publication `/f/:slug`

**Branche** : `feat/014_publish`

| Commit | Contenu |
|:---|:---|
| `feat(api): GET /f/:slug → SSR published revision` | — |
| `feat(api): POST /api/funnels/:id/publish` | Atomique |
| `feat(api): POST /api/funnels/:id/unpublish` | — |
| `feat(render): public route uses render_manifest` | — |
| `feat(api): Cache-Control + ETag` | Edge cache |
| `test(api): publish + access /f/:slug` | — |
| `test(api): unpublished returns 404` | — |

**Critère** : `curl /f/mon-funnel` → HTML rendu.

---

### Phase 5 — Interfaces (semaines 8-9)

#### PR-015 — Command registry

**Branche** : `feat/015_commands`

| Commit | Contenu |
|:---|:---|
| `feat(commands): dispatcher (actor=local_user)` | — |
| `feat(commands): 15 commandes` | funnel.*, section.*, theme.*, publish.* |
| `feat(commands): idempotency keys` | — |
| `feat(commands): error taxonomy` | C-04 |
| `test(commands): dispatch tests` | — |

---

#### PR-016 — CLI

**Branche** : `feat/016_cli`

| Commit | Contenu |
|:---|:---|
| `feat(cli): eez config set endpoint <url>` | — |
| `feat(cli): eez funnels list` | — |
| `feat(cli): eez funnels create <name>` | — |
| `feat(cli): eez funnels publish <id>` | — |
| `feat(cli): eez deploy` | wrangler wrapper |
| `test(cli): integration tests` | — |
| `ci: release workflow (linux, macos, windows)` | — |

---

#### PR-017 — MCP server `/mcp`

**Branche** : `feat/017_mcp`

| Commit | Contenu |
|:---|:---|
| `feat(mcp): rmcp 1.4.0+ server (HTTP transport)` | — |
| `feat(mcp): bearer token auth (EEZ_MCP_TOKEN)` | Secret env |
| `feat(mcp): tool execute_command` | Dispatcher universel |
| `feat(mcp): tool list_commands` | Découverte |
| `feat(mcp): tool list_funnels, get_funnel` | Read |
| `feat(mcp): strict Host validation` | — |
| `test(mcp): bearer token rejected without header` | 401 |
| `test(mcp): execute_command from Claude Desktop` | Smoke test |

**Critère** : Claude Desktop peut créer un funnel via `execute_command("funnel.create", {...})`.

---

### Phase 6 — Documentation et release (semaine 10)

#### PR-018 — Documentation

**Branche** : `docs/018_docs`

| Commit | Contenu |
|:---|:---|
| `docs: README polish (quickstart, demo GIF, features)` | — |
| `docs: docs/getting-started.md` | — |
| `docs: docs/deploy.md` | Pas à pas wrangler |
| `docs: docs/commands.md` | Référence |
| `docs: docs/mcp.md` | Setup Claude Desktop |
| `docs: docs/themes.md` | Créer un thème |
| `docs: docs/security.md` | Cloudflare Access optionnel |
| `docs: examples/` | 3 funnels JSON complets |

---

#### PR-019 — Tests E2E + qualité

**Branche** : `test/019_e2e`

| Commit | Contenu |
|:---|:---|
| `test(e2e): full parcours (create → edit → publish → visit)` | Playwright |
| `test(a11y): axe-core sur dashboard + funnel publié` | 0 critique |
| `test(a11y): navigation clavier complète` | — |
| `test(fuzz): JSON parsing (10 000 entrées)` | Aucun panic |
| `test(load): 100 funnels, 1000 requêtes /f/:slug` | P95 < 100ms |

---

#### PR-020 — Release v0.1.0

**Branche** : `release/v0.1.0`

| Commit | Contenu |
|:---|:---|
| `chore: bump version to 0.1.0` | — |
| `docs: CHANGELOG.md` | Notes v0.1.0 |
| `chore: tag v0.1.0` | Git tag |
| `docs: GitHub Release notes` | — |
| `ci: publish binaires CLI (linux, macos, windows)` | — |

---

## 7. Critère de DONE v0.1.0

- [ ] `wrangler deploy` sur compte Cloudflare vierge fonctionne
- [ ] Ouvrir l'URL → dashboard (aucune auth)
- [ ] Créer, éditer, publier un funnel
- [ ] `/f/:slug` accessible publiquement
- [ ] MCP répond à `execute_command` avec bearer token
- [ ] CLI : `eez funnels list`, `eez deploy` fonctionnent
- [ ] Binaires CLI téléchargeables (linux, macos, windows)
- [ ] README + docs + 3 exemples complets
- [ ] CI verte sur main
- [ ] MIT
- [ ] axe-core 0 critique
- [ ] Aucun `unwrap`/`panic` sur chemin de production
- [ ] 10 000 fuzz inputs sans panic
- [ ] `plan/STATUS.md` indique `DONE`

---

## 8. Estimation

| Phase | PRs | Durée agent |
|:---|:---|:---|
| Bootstrap | 2 | 2 jours |
| Runtime Cloudflare | 2 | 3 jours |
| Modèle + rendu | 4 | 4 jours |
| Dashboard + éditeur | 6 | 8 jours |
| Publication | 1 | 2 jours |
| Interfaces | 3 | 4 jours |
| Docs + release | 3 | 3 jours |
| **Total** | **21 PRs** | **~26 jours agent** |

---

## 9. Ce que l'utilisateur final voit

1. **Owner** : clone le repo, `wrangler deploy`, obtient `https://eezfunnel-xyz.workers.dev`
2. **Owner ou visiteur** : ouvre l'URL → **dashboard direct** (aucune auth, aucun signup)
3. Crée un funnel "Tripwire" → 3 sections pré-remplies apparaissent
4. Édite le hero : titre, sous-titre, CTA, image. Canvas se met à jour en direct
5. Drag-drop pour réordonner. Ctrl+Z si erreur
6. Passe en viewport mobile (375 px), ajuste
7. Change de thème → tout le funnel se recolore instantanément
8. Clique "Publier" → `/f/mon-funnel` devient accessible
9. Ouvre `https://eezfunnel-xyz.workers.dev/f/mon-funnel` → le funnel est là
10. Dans Claude Desktop : *"Crée un funnel nommé 'webinar-foo' avec un hero et un CTA"* → MCP répond, funnel créé

**Expérience** : pas d'auth, pas d'inscription. Tu déploies, tu es sur le dashboard.

---

## 10. Récapitulatif

| Élément | Valeur |
|:---|:---|
| Nom | Eez Funnel |
| Repo | `github.com/eezfunnel/eezfunnel` |
| Licence | MIT |
| Type | Self-hosted SaaS single-user |
| Stack | Rust + Axum + Cloudflare Workers + D1 + R2 + KV |
| Frontend | Dioxus WASM |
| MCP | rmcp 1.4+ (HTTP, Bearer token) |
| PRs | 21 (PR-000 → PR-020) |
| Durée | ~26 jours agent |
| Scope | Dashboard + éditeur + publication + MCP |
| Hors scope | Auth, landing, sous-domaines, CRM, analytics, paiements, IA |

---

## 11. Prochaine action

**Lancer PR-000** :

1. Créer `eezfunnel/eezfunnel` sur GitHub (public, MIT)
2. Copier tous les fichiers fondateurs (bloc unique fourni précédemment)
3. `cargo xtask check` vert
4. Premier commit avec `Task-ID: PR-000`
5. Enchaîner PR-001 → PR-020

**Le plan est complet, exécutable de A à Z sans intervention.** Toutes les valeurs sont gelées, toutes les décisions sont actées, tous les artefacts sont templatés. Aucun gate humain. Aucune alternative non tranchée.

Fin du plan final.