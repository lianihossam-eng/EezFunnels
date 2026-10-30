# Preserve canonical documents with explicit renderer capabilities

Status: accepted for the public improvement series.

Keep the document and schema at V2 for additive optional fields, and omit absent fields during serialization so existing canonical bytes remain unchanged. Use an explicit renderer capability table to preserve renderer v3 through v7 behavior; add renderer-v8 for locale and form presentation, then introduce separate capabilities for new block and action families. Reject a publish or save that requires an unsupported capability instead of silently dropping content. This keeps existing revisions readable while allowing the product to add typed content without pretending old renderers understand it.

The v3–v8 capability contract is exact: v3 renders the entry page only; v4 adds multipage routes; v5 adds native forms; v6 adds digest-bound images; v7 adds SEO metadata; v8 adds an optional public language, form presentation, and multiline presentation for existing text fields. The published route keeps the historical v3/v4 form rejection at render time. Renderer profiles are consulted for both capability validation and output routing so a new renderer version cannot shift older feature sets.

Public language is optional in the V2 document and resolves to English when absent. Missing additive fields remain omitted from canonical serialization. Changing a document's public language changes renderer-owned text and defaults for newly created content; it does not translate content or consent text authored earlier. Multiline text normalizes CRLF and CR to LF before validation, recovery, persistence, and retry fingerprinting while keeping the existing text submission type and scalar-count limits.
