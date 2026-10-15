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

Copy the D1 `database_id` and KV namespace `id` printed by Wrangler into the top-level `DB` and `KV` entries in `wrangler.toml`. Keep the R2 bucket name aligned with the configured `R2` binding. The Worker currently requires all three bindings; image upload is not implemented.

Edit the top-level `[vars]` entry in `wrangler.toml` and set `ALLOWED_HOSTS` to the exact hostname that will receive requests. For the default workers.dev route, this is the Worker name and your account's workers.dev subdomain, for example `eezfunnel.<account-subdomain>.workers.dev`. The Worker rejects hosts that are not listed exactly. Multiple hosts can be comma-separated. If you use another hostname, configure its Cloudflare Worker route separately and list that exact host.

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

A healthy Worker responds with `{"status":"ok"}`. Then open the same host in a browser and create a test funnel. Publish it from the editor workflow or with `eez funnels publish <funnel-id>`; its public page is available at `/f/<slug>`.

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
