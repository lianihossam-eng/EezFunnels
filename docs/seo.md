# Page search and sharing settings

In the typed editor, select a page and open **Page search and sharing**. Each page has independent settings:

- **SEO title** overrides the HTML title and Open Graph title. Leave it blank to use the page title. It does not rename the page in navigation.
- **Meta description** sets both the description and Open Graph description. Leave it blank to omit both tags.
- **Ask search engines not to index this page** adds a `robots` meta tag with `noindex`.

Titles allow up to 120 Unicode scalar values; descriptions allow up to 320. Whitespace is trimmed and collapsed into single spaces. Unsupported control characters and values exceeding these limits are rejected without truncation.

Save a revision, then publish it to update the public page. The owner preview uses the same metadata rules. Published metadata is escaped for HTML text and attributes. New revisions also include Open Graph type `website`.

`noindex` is an indexing preference for cooperating search engines. It does not restrict access: anyone with a public page URL can still open the page. Use an access perimeter when content must be protected; see [Security and Cloudflare Access](security.md).

Published pages also emit a self-referencing canonical link when `PUBLIC_BASE_URL` is configured as an HTTPS origin, such as `https://funnels.example`. The value must contain only the origin (an optional trailing slash is accepted); a missing or invalid value omits the canonical link. The link uses the public funnel/page route and excludes request query parameters. It never trusts `Host` or forwarded-host request headers. When several hosts serve the same Worker, this one configured origin is canonical for all of them. A page marked `noindex` remains excluded by its robots directive.

`GET /sitemap.xml` lists routes from each funnel's current published revision. It includes V1 entry pages and indexable V2 pages, and excludes unpublished funnels, draft revisions, and V2 pages marked `noindex`. URLs use only the validated `PUBLIC_BASE_URL`; when it is unset or invalid, the sitemap is empty. The sitemap does not use request `Host` or forwarded-host headers.

`GET /robots.txt` allows crawlers to read published funnel pages and their public assets while discouraging crawlers from indexing the dashboard, owner APIs, submissions, and MCP endpoint. It points to the sitemap only when `PUBLIC_BASE_URL` is valid. `robots.txt` rules and page `noindex` are crawler hints, not access controls. Anyone who needs to keep a published page or owner route private must protect it with Cloudflare Access or another network perimeter.

Earlier revisions keep their renderer behavior. Renderer v4 and v5 revisions have title-only metadata; v6 also supports images. Existing revisions remain renderable, while newly saved revisions use renderer v7. Metadata is tied to the saved document and validated against its render manifest.

Social-image tags are not emitted. Canonical links are added by the public API edge from the operator-configured `PUBLIC_BASE_URL`; the renderer does not infer a trusted canonical origin from request headers.
