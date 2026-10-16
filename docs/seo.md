# Page search and sharing settings

In the typed editor, select a page and open **Page search and sharing**. Each page has independent settings:

- **SEO title** overrides the HTML title and Open Graph title. Leave it blank to use the page title. It does not rename the page in navigation.
- **Meta description** sets both the description and Open Graph description. Leave it blank to omit both tags.
- **Ask search engines not to index this page** adds a `robots` meta tag with `noindex`.

Titles allow up to 120 Unicode scalar values; descriptions allow up to 320. Whitespace is trimmed and collapsed into single spaces. Unsupported control characters and values exceeding these limits are rejected without truncation.

Save a revision, then publish it to update the public page. The owner preview uses the same metadata rules. Published metadata is escaped for HTML text and attributes. New revisions also include Open Graph type `website`.

`noindex` is an indexing preference for cooperating search engines. It does not restrict access: anyone with a public page URL can still open the page. Use an access perimeter when content must be protected; see [Security and Cloudflare Access](security.md).

Earlier revisions keep their renderer behavior. Renderer v4 and v5 revisions have title-only metadata; v6 also supports images. Existing revisions remain renderable, while newly saved revisions use renderer v7. Metadata is tied to the saved document and validated against its render manifest.

Canonical URL and social-image tags are not emitted. The renderer does not infer a trusted canonical origin from request headers.
