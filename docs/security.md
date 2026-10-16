# Security

Eez Funnel v0.1 is a single-user application and does not include a login, user accounts, or per-user roles. A public Worker hostname therefore exposes the administrative dashboard and funnel API unless you put an access policy in front of those routes. The Worker `ALLOWED_HOSTS` setting is an exact host allowlist; it is not authentication.

## Protect administrative routes

Before making the Worker reachable from the public Internet, use Cloudflare Access or another network perimeter to require authentication for the dashboard and API routes, including `/`, `/funnels`, `/media`, `/funnels/*`, `/api/funnels` (including its subroutes and the owner `/api/funnels/:funnel_id/leads*` and `/api/funnels/:funnel_id/analytics*` read routes), and `/api/media` (including its subroutes). Protect `/mcp` as well if you enable it. Static dashboard assets under `/assets/*` can be public.

Published entry pages use `GET /f/:slug`; additional V2 pages use `GET /f/:slug/:page_slug`. Add an Access bypass/public policy for those published-page paths only if those funnels should be public. If the bypass is omitted, visitors will also need to authenticate before seeing a published funnel.

Published forms post to `/api/public/funnels/:slug/pages/:page_id/forms/:form_id/submissions`. If a funnel is public, allow this specific route to receive same-origin POST requests as well as allowing its published-page paths. Do not make the dashboard or `/api/funnels/*` public to enable form submissions. Form values and consent evidence are stored in D1 for 90 days by default; see [Public forms](forms.md) for retention and rate-limit settings. The read-only lead routes expose stored lead data; the form analytics route exposes aggregate owner data. Keep all of them behind the same external Access perimeter as the dashboard. Responses are non-cacheable, but `Cache-Control: no-store` does not replace access control.

Published image blocks load from `/media/:asset_id`. Allow that asset route for public funnels while keeping the exact `/media` dashboard route and `/api/media*` owner routes protected. The Worker returns an image only if a currently published revision references its digest; archiving does not break a published page. Upload and R2 capacity limits are documented in [Media library](media.md).

Access is an edge policy, not application login or multi-user authorization. The native `eez` CLI currently has no support for Access cookies or arbitrary Access headers; funnel commands through an Access-protected API will not work with the current CLI. An MCP client must be able to send any required Access credentials as well as the Eez Bearer token. The Worker does not provide a second authentication mode for these tools. `ALLOWED_HOSTS` checks the request host only and does not authenticate callers of owner read APIs.

## MCP token

The `/mcp` route requires a high-entropy `EEZ_MCP_TOKEN` Worker secret and compares the `Authorization: Bearer` value. Configure it with `wrangler secret put EEZ_MCP_TOKEN`; do not store it in `wrangler.toml`, repository files, issue text, or shell history. Missing server configuration fails closed. Rotate the Worker secret and update client configurations if it is exposed.

The MCP endpoint also checks the exact allowed `Host`. If an `Origin` header is present, it must be one valid origin matching the request scheme and host. Request bodies, tool arguments, and tokens are not included in application error bodies.

## Funnel content and limits

Section text is escaped for HTML output. Link URLs must be HTTPS or same-worker root-relative URLs; unsafe schemes and malformed URLs are rejected. V2 images come from the verified R2 media library and do not accept user-provided URLs. The renderer does not accept user-provided HTML, JavaScript, CSS, or event handlers.

The Worker applies request-size and content limits, and saves immutable revisions with compare-and-set publication. Mutation keys protect retries for 24 hours; reuse the same key and parameters for a retry. The current configuration limits a workspace to 100 funnels and retains up to 100 revisions per funnel. These limits do not replace backups.

The native CLI stores its endpoint and immutable retry tickets in the user config directory. Tickets contain the endpoint, route, and request data, including funnel names, but no credentials. They remain after a successful request and expire after 23 hours. Keep the directory private. A Claude Desktop configuration that uses a local bridge may also store the MCP token in clear text; protect that file and do not commit it.

See [SECURITY.md](../SECURITY.md) for vulnerability reporting and the supported-version policy. Deployment and Access setup are covered in [Deploy](deploy.md).
