# Commands reference

The dashboard, HTTP API, CLI, and MCP endpoint use the same funnel command session where implemented. Command IDs are stable names. The HTTP API exposes specific routes; it does not provide a general `/api/commands` route.

## Stored snapshot examples

[`examples/webinar.json`](../examples/webinar.json), [`examples/vsl.json`](../examples/vsl.json), and [`examples/tripwire.json`](../examples/tripwire.json) show complete version 1 revision snapshots in the same shape stored in a revision's `data_json` field. Their `example.com` URLs and sample testimonial text are placeholders to replace before publishing. Funnel IDs, names, publication pointers, and render manifests are stored separately. The current product has no JSON import or export feature, so these files are reference examples; they are not accepted as `funnel.create` bodies or uploaded through an API route. Create a funnel through the dashboard, CLI, or `funnel.create` command instead.

## HTTP routes

| Method and path | Behavior |
|:---|:---|
| `GET /api/funnels` | List workspace funnels (`funnel.list`) |
| `POST /api/funnels` | Create from a blueprint (`funnel.create`); requires an `Idempotency-Key` header |
| `POST /api/funnels/v2` | Create a typed version 2 funnel from a blueprint; requires an `Idempotency-Key` header |
| `GET /api/funnels/:funnel_id` | Read the editable funnel and current revision (`funnel.editor.read`) |
| `DELETE /api/funnels/:funnel_id` | Delete the funnel and its revisions (`funnel.delete`); requires an `Idempotency-Key` header |
| `POST /api/funnels/:funnel_id/preview` | Render the proposed editor state without saving (`funnel.editor.preview`) |
| `POST /api/funnels/:funnel_id/revisions` | Save a new immutable revision (`funnel.revision.save`); requires an `Idempotency-Key` header |
| `POST /api/funnels/:funnel_id/publish` | Publish a revision if the expected pointer still matches (`funnel.publish`); requires an `Idempotency-Key` header |
| `POST /api/funnels/:funnel_id/unpublish` | Clear the published pointer if the expected revision still matches (`funnel.unpublish`); requires an `Idempotency-Key` header |
| `GET /api/funnels/:funnel_id/document[?revision_id=...]` | Read the current typed document or select a typed revision |
| `GET /api/funnels/:funnel_id/revisions-v2` | Read the funnel's revision history |
| `POST /api/funnels/:funnel_id/preview-v2` | Render a typed document preview |
| `POST /api/funnels/:funnel_id/edit-v2` | Apply a typed page, section, or block edit |
| `POST /api/funnels/:funnel_id/convert-v2/preview` | Preview conversion from a version 1 funnel and report blockers |
| `POST /api/funnels/:funnel_id/convert-v2` | Save a validated conversion as a new typed revision; requires an `Idempotency-Key` header |
| `POST /api/funnels/:funnel_id/stages-v2` | Create a bounded staged upload; requires an `Idempotency-Key` header |
| `PUT /api/funnels/:funnel_id/stages-v2` | Upload staged document bytes with the returned bearer capability |
| `POST /api/funnels/:funnel_id/revisions-v2` | Validate and save a staged typed revision; requires an `Idempotency-Key` header |
| `GET /api/v2/stage-capacity` | Read the configured staged-save capacity |
| `GET /api/media` | List active or archived media in pages of 50 |
| `POST /api/media` | Upload bounded raw JPEG/PNG image bytes |
| `POST /api/media/:asset_id/archive` | Archive or restore an image without deleting it |
| `GET /api/media/:asset_id/content` | Read image bytes for owner previews |
| `GET /api/media/capacity` | Read the configured media R2 allowance |
| `GET /media/:asset_id` | Read an image referenced by a currently published revision |
| `GET /f/:slug` | Render the published entry page (`funnel.public.read`) |
| `GET /f/:slug/:page_slug` | Render a published V2 page (`funnel.public.read`) |

JSON mutation requests use JSON bodies. The `PUT` staged upload and `POST /api/media` image upload use `application/octet-stream`; staged uploads also require a short-lived bearer capability. Errors have `code`, `category`, `message`, and `correlation_id` fields. The [Security guide](security.md) describes request limits and host checks; [Deploy](deploy.md) describes the staged-save and media capacity settings.

## MCP commands

The MCP endpoint exposes the following nine command IDs. Typed editor operations use the HTTP routes above and are not exposed through MCP.

| ID | Parameters | Actor | Idempotency key |
|:---|:---|:---|:---|
| `funnel.create` | `{"name":"Webinar","blueprint":"webinar"}` | Local user | Required |
| `funnel.delete` | `{"funnel_id":"<id>"}` | Local user | Required |
| `funnel.editor.preview` | `funnel_id`, `base_revision_id`, `section_ids`, optional `settings` | Local user | Not required |
| `funnel.editor.read` | `{"funnel_id":"<id>"}` | Local user | Not required |
| `funnel.list` | `{}` | Local user | Not required |
| `funnel.public.read` | `{"slug":"<published-slug>","page_slug":"<page-slug>"}` (`page_slug` optional for the entry page) | Public | Not required |
| `funnel.publish` | `funnel_id`, `revision_id`, `expected_published_revision_id` | Local user | Required |
| `funnel.revision.save` | `funnel_id`, `base_revision_id`, `section_ids`, optional `settings` | Local user | Required |
| `funnel.unpublish` | `funnel_id`, `expected_published_revision_id` | Local user | Required |

The supported blueprint IDs are `tripwire`, `vsl`, `webinar`, `launch`, and `lead_gen`. Funnel names are trimmed, must contain non-whitespace text, and are limited to 120 UTF-8 bytes. Funnel and revision IDs are supplied by the application and should be copied exactly from its responses.

For publish, `expected_published_revision_id` is required and may be `null` when the funnel has never been published. The command uses it as a compare-and-set guard. For unpublish, it must be the current published revision ID. If another request changes that pointer first, reload the funnel and try again with the current value.

For editor preview and save, `section_ids` must be an exact permutation of the current section IDs. These commands reorder existing sections; they do not add or remove sections. Save creates a new revision and requires an idempotency key.

The optional editor `settings` object has `schema_version: 1` and a `changes` array. Each change has a field `id` and a typed value. For example:

```json
{
  "schema_version": 1,
  "changes": [
    {"id":"theme.preset","value":{"kind":"select","value":"elegant"}},
    {"id":"section.hero.title","value":{"kind":"text","value":"A clearer offer"}}
  ]
}
```

Field IDs depend on the funnel's sections and are returned with the editor read response in `settings_schema` and `settings_values`. Current control kinds are `text`, `color`, `select`, `toggle`, and `image`; the editor only accepts fields described by that schema.

## Idempotency

For `funnel.create`, `funnel.delete`, `funnel.publish`, `funnel.revision.save`, and `funnel.unpublish`, the HTTP API requires an `Idempotency-Key` header. The MCP `execute_command` tool accepts the equivalent `idempotency_key` argument. Keys may contain 1–128 ASCII letters, digits, `.`, `_`, or `-`. A UUID is a convenient choice.

Reuse the exact key and payload when retrying an uncertain mutation. The Worker stores mutation receipts for 24 hours; using the same key with different parameters is a conflict. Start a new user action with a new key. The native CLI creates and stores its own retry ticket before sending create or publish requests; those local tickets expire after 23 hours. See the [CLI README](../apps/cli/README.md) for the ticket location and behavior.

## Native CLI

Build the CLI with `cargo build --release -p eez-cli`, then configure an API endpoint:

```sh
eez config set endpoint https://eezfunnel.<account-subdomain>.workers.dev
eez funnels list
eez funnels create 'October webinar' --blueprint webinar
eez funnels publish <funnel-id>
eez funnels retry <operation-uuid>
```

`eez config set endpoint http://localhost:8787` is accepted for local development. Remote endpoints must use HTTPS. The CLI supports list, create, publish, and retry; it does not expose delete or editor controls. Create defaults to the `tripwire` blueprint when `--blueprint` is omitted.

The CLI writes endpoint configuration and retry tickets to the platform's user config directory. Set `EEZ_CONFIG_DIR` to select another directory. Tickets include the original endpoint, request route, and request body; keep the local config directory private. The CLI does not store credentials.

`eez deploy` runs `wrangler deploy` in the caller's current directory and forwards Wrangler's output and exit status. It requires Wrangler on `PATH`; it does not build the caller's project. See [Deploy](deploy.md).
