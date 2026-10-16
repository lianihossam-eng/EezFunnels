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

Copy the D1 `database_id` and KV namespace `id` printed by Wrangler into the top-level `DB` and `KV` entries in `wrangler.toml`. Keep the R2 bucket name aligned with the configured `R2` binding. The Worker requires all three bindings.

Edit the top-level `[vars]` entry in `wrangler.toml` and set `ALLOWED_HOSTS` to the exact hostname that will receive requests. For the default workers.dev route, this is the Worker name and your account's workers.dev subdomain, for example `eezfunnel.<account-subdomain>.workers.dev`. The Worker rejects hosts that are not listed exactly. Multiple hosts can be comma-separated. If you use another hostname, configure its Cloudflare Worker route separately and list that exact host.

## V2 staged-save capacity

V2 funnel creation and staged saves require explicit production values for `V2_STAGE_ORPHAN_BYTE_BUDGET` and `V2_STAGE_ORPHAN_KEY_BUDGET` in the top-level `[vars]` table. The repository intentionally supplies no production capacity. Without both positive values, V2 creation fails closed. Local development has separate explicit limits in `[env.dev.vars]`.

Each R2 upload attempt permanently charges its expected byte length and one object key in the external stage-control object before the R2 PUT. Cleanup, failed or timed-out writes, and D1 restore never refund that charge. These lifetime limits bound worst-case orphan exposure even when an object is never observed or removed. The typed editor displays the configured worst-case byte/key exposure and the amount already charged; `GET /api/v2/stage-capacity` provides the same status to the dashboard.

Set an initial cap appropriate for the account before creating V2 funnels. Increasing either cap is an explicit operator change and the editor displays the resulting maximum exposure; the persisted limit only increases. If configuration is lowered below the persisted cap, V2 staging fails closed. The control object is initialized on the first V2 create and lives in R2 outside D1 Time Travel restores.

## Media storage capacity

`MEDIA_R2_BYTE_BUDGET` and `MEDIA_R2_KEY_BUDGET` bound content-addressed media writes. Production defaults are 1 GiB and 10,000 write slots. Local development has separate 100 MiB and 1,000 slot limits. The media library displays the configured and used allowance. Each new R2 media write permanently charges its normalized bytes and one slot before the PUT. Failures, uncertain retries, and D1 restores do not refund charges. Increase capacity explicitly when the allowance is reached; the persisted R2 limit only increases. See [Media library](media.md) for upload formats, routes, and archive behavior.

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

A healthy Worker responds with `{"status":"ok"}`. Then open the same host in a browser and create a test funnel. The editor provides publish and unpublish controls for saved revisions; the entry page is available at `/f/<slug>`, and additional published V2 pages use `/f/<slug>/<page_slug>`. Review the configured V2 staging and media capacities before enabling those features.

## Optional MCP secret

To enable the `/mcp` endpoint, set a high-entropy token as a Worker secret. Wrangler prompts for the value:

```sh
npx wrangler secret put EEZ_MCP_TOKEN
```

The MCP route fails closed when this secret is missing. Do not put the token in `wrangler.toml`, source control, or a command saved in shell history. See [MCP setup](mcp.md).

## CLI deployment

The native CLI can forward a Wrangler deployment command from the current directory:

```sh
cd /path/to/EezFunnels
/path/to/eez deploy
```

It runs `wrangler deploy`, inherits Wrangler's output, and returns its exit code. It does not build the CLI's caller's project or change the configured funnel API endpoint. Wrangler must be on `PATH` and authenticated to the intended account.

No remote migration or deployment is part of the repository's local verification. See [Security](security.md) before making the Worker public.
