# Eez command line

Build `eez` with `cargo build --release -p eez-cli`, then place the binary on your PATH. Funnel commands use the configured Worker API. Configure an HTTPS origin; HTTP is accepted for `localhost` and loopback IP addresses during local development.

```sh
eez config set endpoint https://funnels.example.workers.dev
eez funnels list
eez funnels create 'October offer'
eez funnels create 'Webinar' --blueprint webinar
eez funnels publish <funnel-id>
```

Create defaults to `tripwire`; accepted blueprints are `tripwire`, `vsl`, `webinar`, `launch`, and `lead_gen`. Publish reads the latest saved editor revision and the current published pointer, then performs one compare-and-set request. A stale publication returns the server conflict; rerun publication after inspecting the current state. An already-published head returns `already_published: true` without writing. Successful funnel commands emit a bounded JSON projection of the Worker result.

Each create/publish saves an immutable operation file before sending its request and prints `eez funnels retry <operation-uuid>` to stderr. If a network response is lost, replay this operation to recover its server receipt. Retry pins the original endpoint, route, body, and idempotency key even after config changes; it performs no new editor reads. A new create invocation always creates a fresh operation. Tickets remain on disk after success and can be replayed for less than 23 hours, assuming an accurate system clock. Expired tickets are removed on a best-effort basis when a retry is attempted or before a later mutation is saved. Inspect the server state before starting a new operation when an older ticket has expired.

The config file and tickets live in the platform user config directory (`eez` under Linux XDG config, macOS Application Support, or Windows roaming AppData). Set `EEZ_CONFIG_DIR` to use a different directory. Endpoints are limited to 2 KiB and the serialized config/tickets to 16 KiB. These files contain endpoint and funnel request data, including names, but no credentials. Linux/macOS persistence syncs files and directories; Windows syncs the file and uses atomic replacement/no-clobber persistence, without directory fsync.

```sh
cd /path/to/worker-project
eez deploy --env production
```

Deploy requires Wrangler on PATH, runs `wrangler deploy` in the caller's directory, inherits its output, and forwards native OS arguments. On Windows the standard npm `wrangler.cmd` installation is launched through `node.exe` and its known `wrangler/bin/wrangler.js` entrypoint; arbitrary custom batch wrappers are unsupported. It requires no source checkout or Cargo toolchain. Deploy forwards the child exit code; signal termination returns 1.

`scripts/smoke-pr016.sh` exercises the built CLI from outside the checkout against an isolated local Worker/D1 instance. It performs no remote migration or deployment.
