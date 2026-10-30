# Preserve canonical documents with explicit renderer capabilities

Status: accepted for the public improvement series.

Keep the document and schema at V2 for additive optional fields, and omit absent fields during serialization so existing canonical bytes remain unchanged. Use an explicit renderer capability table to preserve renderer v3 through v7 behavior; add renderer-v8 for locale and form presentation, then introduce separate capabilities for new block and action families. Reject a publish or save that requires an unsupported capability instead of silently dropping content. This keeps existing revisions readable while allowing the product to add typed content without pretending old renderers understand it.
