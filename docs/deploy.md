# Deploy to Cloudflare

This guide deploys the Worker from the repository root. It creates remote resources and applies production D1 migrations; review the target account and database before running those commands. The checked-in production D1 and KV identifiers are placeholders, so deployment requires your account's resource IDs.

## Prerequisites

- A Cloudflare account with Workers, D1, R2, and Workers KV available.
- Node.js 22 or newer, npm, and the pinned Rust toolchain.
- Wrangler installed from `package-lock.json` and authenticated to the intended account.
- The pinned `worker-build` CLI. Install it with `./scripts/install-worker-build.sh`.

From the repository root:

```sh
npm ci
./scripts/install-worker-build.sh
npx wrangler login
```

The `wrangler.toml` build command installs `wasm-bindgen` 0.2.129 when needed, builds the dashboard assets, and builds the Worker.

## Create and bind production resources

Create the resources named by the production configuration:

```sh
npx wrangler d1 create eezfunnel
npx wrangler r2 bucket create eezfunnel-assets
npx wrangler kv namespace create eezfunnel-kv
```

Copy the D1 `database_id` and KV namespace `id` printed by Wrangler into the top-level `DB` and `KV` entries in `wrangler.toml`. Keep the R2 bucket name aligned with the configured `R2` binding. The Worker requires all three bindings. The media library stores validated PNG, JPEG, and WebP uploads in R2; uploaded images are publicly retrievable at immutable content-addressed URLs, so do not upload private images.

Edit the top-level `[vars]` entry in `wrangler.toml` and set `ALLOWED_HOSTS` to the exact platform hostname that will receive dashboard and owner API requests. For the default workers.dev route, this is the Worker name and your account's workers.dev subdomain, for example `eezfunnel.<account-subdomain>.workers.dev`. Multiple platform hosts can be comma-separated.

Set `PUBLIC_BASE_URL` in the same `[vars]` table to the public HTTPS origin for funnels without a custom funnel hostname, for example `https://eezfunnel.<account-subdomain>.workers.dev`. Do not include a path, query, or fragment. This explicit value supplies canonical links and sitemap URLs for those funnels; if it is unset or invalid, their canonical links are omitted and the platform sitemap is empty.

To serve a funnel from an additional hostname in your own active Cloudflare zone, first provision that exact hostname as a Worker Custom Domain for this Worker using Wrangler, the Cloudflare dashboard, or your infrastructure-as-code setup. Cloudflare manages DNS and TLS for the same-zone hostname. Worker Custom Domains require an operator-controlled active zone, do not support a pre-existing CNAME, and do not provide wildcard hostnames. EezFunnels does not call the Cloudflare API or need a Cloudflare token.

Then set `PUBLIC_FUNNEL_HOSTS` to comma-separated `hostname=funnel-slug` entries, for example `funnels.example=lead-gen,offers.example=summer-offer`. Use lowercase public funnel slugs and exact DNS hostnames without schemes, paths, wildcards, or ports. Do not repeat a hostname or funnel slug, and do not map a hostname listed in `ALLOWED_HOSTS` or used by `PUBLIC_BASE_URL`. An invalid or conflicting mapping disables all custom funnel host routing. The mapped hostname serves the published entry page at `/` and pages at their root slug; the existing `/f/<slug>` paths remain available there for links produced by published documents. Only the mapped funnel's public pages, its published media, and its public form submission route are served on that host. The dashboard, owner APIs, MCP, admin, and local-only routes are unavailable there.

The mapped hostname becomes canonical for that funnel. Its sitemap lists only that funnel's current published, indexable pages; the platform sitemap omits mapped funnels. In addition to pages, referenced media, and form posts, a mapped host serves the first-party interaction script and only that funnel's interaction-event POST route. `robots.txt` is a crawler hint, not an access control. Do not place private content in a published funnel or rely on `robots.txt` or `noindex` to protect it.

## V2 staged-save capacity

The top-level `[vars]` table supplies initial caps of 104,857,600 bytes (100 MiB) and 1,000 object keys through `V2_STAGE_ORPHAN_BYTE_BUDGET` and `V2_STAGE_ORPHAN_KEY_BUDGET`. These are starting values; review them against the account and the exposure you accept before deployment. Local development has separate values in `[env.dev.vars]`. If either variable is missing, invalid, or non-positive, V2 staging and saves fail closed. The dashboard keeps an already saved V2 document readable and disables saves while the capacity endpoint is unavailable.

Before an upload, the Worker reserves its expected byte length and one object key across active stage attempts. The reservation is released when the stage is consumed or expired and its object cleanup completes; it is not a lifetime charge. The external stage-control object keeps the accepted caps outside D1 so a D1 restore does not reset them. The typed editor shows the byte/key caps and currently held reservations; `GET /api/v2/stage-capacity` provides the same status to the dashboard.

Increasing either persisted cap is an explicit operator change; the persisted limits only increase. If configuration is lowered below the persisted cap, V2 staging fails closed. The control object is initialized on the first V2 create and lives in R2 outside D1 Time Travel restores.

Do not deploy with the placeholder D1 or KV IDs. The `[env.dev]` resources are separate from the production bindings; the local development commands do not need real remote dev resources.

## Apply migrations and deploy

Check that Wrangler is using the intended account, then apply migrations to the remote production database and deploy:

```sh
npx wrangler whoami
npx wrangler d1 migrations apply eezfunnel --remote
npx wrangler deploy
```

The remote migration command changes the selected D1 database. Do not add `--remote` to local development migration commands unless you intend to change the Cloudflare database.

Confirm the health route and dashboard after deployment:

```sh
curl --fail --show-error https://eezfunnel.<account-subdomain>.workers.dev/health
```

A healthy Worker responds with `{"status":"ok"}`. Then open the same host in a browser and create a test funnel. The editor provides publish and unpublish controls for saved revisions; the entry page is available at `/f/<slug>`, and additional published V2 pages use `/f/<slug>/<page_slug>`. Review the configured V2 staging capacity before enabling V2 creation.

## Optional MCP secret

To enable the `/mcp` endpoint, set a high-entropy token as a Worker secret. Wrangler prompts for the value:

```sh
npx wrangler secret put EEZ_MCP_TOKEN
```

The MCP route fails closed when this secret is missing. Do not put the token in `wrangler.toml`, source control, or a command saved in shell history. See [MCP setup](mcp.md).

## Optional interaction measurement secret

To enable the consent choice and first-party interaction collector on published pages, set an installation-specific random key of at least 32 bytes as a Worker secret. This command supplies 32 random bytes encoded as hex without writing the value into shell history:

```sh
openssl rand -hex 32 | npx wrangler secret put INTERACTION_REPLAY_KEY
```

Without this secret, published pages do not load the interaction script and the collector fails closed. Keep the secret unchanged while event rows created with it may remain (up to 90 days); replay-key rotation is not automated. Do not put the key in `wrangler.toml`, source control, issue text, or shell history.

`PUBLIC_INTERACTION_RETENTION_DAYS` defaults to 30 and accepts 1 through 90. `PUBLIC_INTERACTION_EVENTS_PER_MINUTE` defaults to 100 per funnel. These settings are in the top-level `[vars]` table and the separate `[env.dev.vars]` table. The local `dev` environment can use a private `.dev.vars` file for `INTERACTION_REPLAY_KEY`; do not commit that file. The scheduled cleanup removes expired event rows every 15 minutes. See [Interaction measurement](analytics.md) for the consent and data boundaries; no aggregate owner report is included yet.

## CLI deployment

The native CLI can forward a Wrangler deployment command from the current directory:

```sh
cd /path/to/EezFunnels
/path/to/eez deploy
```

It runs `wrangler deploy`, inherits Wrangler's output, and returns its exit code. It does not build the CLI's caller's project or change the configured funnel API endpoint. Wrangler must be on `PATH` and authenticated to the intended account.

No remote migration or deployment is part of the repository's local verification. See [Security](security.md) before making the Worker public.
