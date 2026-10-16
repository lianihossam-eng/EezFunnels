use eez_domain::document_v2::{AssetId, BlueprintId, DocumentV2};
use eez_domain::media::{MediaAssetDescriptor, RasterFormat, ResolvedV2Assets};
use eez_render::document_v2::{
    prepare_revision, render_owner_preview, render_published, validate_revision_v2, V2RenderError,
};

fn document() -> DocumentV2 {
    DocumentV2::starter(
        "funnel-01",
        BlueprintId::LeadGen,
        eez_domain::ThemePreset::Minimal,
    )
    .expect("valid V2 starter")
}

#[test]
fn renderer_v6_manifest_binds_row_document_theme_and_assets() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-v2-01", &ResolvedV2Assets::empty())
        .expect("revision preparation");

    assert_eq!(prepared.manifest.renderer_version(), "renderer-v6");
    assert!(!prepared.render_manifest_json.contains("\"assets\":"));
    assert!(prepared
        .render_manifest_json
        .contains("\"manifest_version\":2"));
    assert_eq!(
        prepared.manifest.document_sha256(),
        sha256(prepared.data_json.as_bytes())
    );

    let html = render_published(
        2,
        "funnel-01",
        "revision-v2-01",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .expect("exact V2 row renders");
    assert!(html.contains("Welcome to your new funnel"));
    assert!(html.contains("--color-background:#f8fafc"));
    assert!(html.contains("--font-family:system-ui, sans-serif"));
}

#[test]
fn owner_read_validates_the_stored_v2_row_and_manifest_without_public_render_gates() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-owner-read", &ResolvedV2Assets::empty())
        .expect("manifest");
    let decoded = validate_revision_v2(
        2,
        "funnel-01",
        "revision-owner-read",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .expect("valid row");
    assert_eq!(decoded, document);
    assert_eq!(
        validate_revision_v2(
            1,
            "funnel-01",
            "revision-owner-read",
            &prepared.data_json,
            &prepared.render_manifest_json,
        )
        .expect_err("wrong row version"),
        V2RenderError::IdentityMismatch
    );
}

#[test]
fn version_identity_digest_and_noncanonical_rows_fail_closed() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-v2-01", &ResolvedV2Assets::empty())
        .expect("revision preparation");
    assert!(matches!(
        render_published(
            1,
            "funnel-01",
            "revision-v2-01",
            &prepared.data_json,
            &prepared.render_manifest_json,
        ),
        Err(V2RenderError::IdentityMismatch)
    ));
    assert!(render_published(
        2,
        "another-funnel",
        "revision-v2-01",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .is_err());
    assert!(render_published(
        2,
        "funnel-01",
        "different-revision",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .is_err());

    let changed = prepared.data_json.replace("Home", "Other");
    assert!(render_published(
        2,
        "funnel-01",
        "revision-v2-01",
        &changed,
        &prepared.render_manifest_json,
    )
    .is_err());

    let noncanonical = format!(" {} ", prepared.data_json);
    assert!(render_published(
        2,
        "funnel-01",
        "revision-v2-01",
        &noncanonical,
        &prepared.render_manifest_json,
    )
    .is_err());

    let wrong_renderer = prepared
        .render_manifest_json
        .replace("renderer-v6", "renderer-v5-next");
    assert!(matches!(
        render_published(
            2,
            "funnel-01",
            "revision-v2-01",
            &prepared.data_json,
            &wrong_renderer,
        ),
        Err(V2RenderError::UnsupportedRenderer)
    ));
}

#[test]
fn legacy_image_free_manifests_remain_canonical_and_renderable() {
    let document = document();
    let prepared = prepare_revision(
        &document,
        "revision-legacy-compatible",
        &ResolvedV2Assets::empty(),
    )
    .expect("image-free revision");

    for renderer in ["renderer-v4", "renderer-v5"] {
        let legacy_manifest = prepared
            .render_manifest_json
            .replace("renderer-v6", renderer);
        let html = render_published(
            2,
            "funnel-01",
            "revision-legacy-compatible",
            &prepared.data_json,
            &legacy_manifest,
        )
        .expect("legacy image-free manifest remains supported");
        assert!(html.contains("Welcome to your new funnel"));
        assert!(!legacy_manifest.contains("\"assets\":"));
    }
}

#[test]
fn owner_preview_escapes_text_and_never_follows_external_links() {
    let document = document();
    let page_id = document.pages()[0].id().clone();
    let edited = document
        .apply(
            eez_domain::document_v2::Edit::SetBlock {
                block: document.pages()[0].sections()[0].blocks()[0].id().clone(),
                value: eez_domain::document_v2::NewBlock::Heading {
                    level: 1,
                    text: "<img src=x onerror=alert(1)>".to_owned(),
                },
            },
            &mut TestIds(0),
        )
        .expect("heading edit");
    let html = render_owner_preview(&edited.document, &page_id, &ResolvedV2Assets::empty())
        .expect("owner preview");
    assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!html.contains("<img src=x"));
}

#[test]
fn resolved_images_render_with_safe_urls_dimensions_and_alt_semantics() {
    let asset_id = "a".repeat(64);
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("starter document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {
            "id": "image-block",
            "kind": "image",
            "asset_id": asset_id,
            "alt": {"kind": "text", "value": "<Product & launch>"}
        }
    ]);
    let image_document = DocumentV2::decode(
        &serde_json::to_vec(&value).expect("image document JSON"),
        "funnel-01",
    )
    .expect("valid image document");
    let descriptor = MediaAssetDescriptor::new(
        AssetId::parse(asset_id.clone()).expect("content digest asset ID"),
        asset_id,
        RasterFormat::Png,
        640,
        640,
        480,
    )
    .expect("validated asset metadata");
    let assets = ResolvedV2Assets::for_document(&image_document, vec![descriptor])
        .expect("exact document asset set");

    let preview = render_owner_preview(&image_document, image_document.entry_page_id(), &assets)
        .expect("image owner preview");
    assert!(preview.contains("src=\"/api/media/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/content\""));
    assert!(preview.contains("alt=\"&lt;Product &amp; launch&gt;\""));
    assert!(preview.contains("width=\"640\" height=\"480\""));

    let prepared = prepare_revision(&image_document, "revision-image-ready", &assets)
        .expect("image revision manifest");
    assert!(prepared.render_manifest_json.contains("\"assets\":["));
    let published = render_published(
        2,
        "funnel-01",
        "revision-image-ready",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .expect("published image renders");
    assert!(published.contains(
        "src=\"/media/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""
    ));
    assert!(published.contains("alt=\"&lt;Product &amp; launch&gt;\""));
}

#[test]
fn v2_images_require_asset_bindings_and_form_previews_are_inert_until_published() {
    let mut image_value: serde_json::Value = serde_json::from_str(
        std::str::from_utf8(document().canonical_bytes()).expect("canonical JSON"),
    )
    .expect("document JSON");
    image_value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"image-block", "kind":"image", "asset_id":"asset-one", "alt":{"kind":"decorative"}}
    ]);
    let image_json = serde_json::to_vec(&image_value).expect("document serialization");
    let image_document = DocumentV2::decode(&image_json, "funnel-01").expect("image node schema");
    let error = prepare_revision(
        &image_document,
        "revision-image",
        &ResolvedV2Assets::empty(),
    )
    .expect_err("an image with no resolved asset must not be saved");
    assert_eq!(error.to_string(), "validation/v2_assets_missing");

    let mut form_value: serde_json::Value = serde_json::from_str(
        std::str::from_utf8(document().canonical_bytes()).expect("canonical JSON"),
    )
    .expect("document JSON");
    form_value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"form-block", "kind":"form", "form_id":"form-one"}
    ]);
    form_value["forms"] = serde_json::json!([{
        "id":"form-one",
        "fields":[{"id":"field-one", "name":"email", "label":"Email", "kind":"email", "required":true}],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
        "result":{"kind":"inline", "message":"Thanks."}
    }]);
    let form_json = serde_json::to_vec(&form_value).expect("document serialization");
    let form_document = DocumentV2::decode(&form_json, "funnel-01").expect("form node schema");
    let preview = render_owner_preview(
        &form_document,
        form_document.entry_page_id(),
        &ResolvedV2Assets::empty(),
    )
    .expect("owner preview renders a form without a live submission target");
    assert!(preview.contains("data-preview=\"true\""));
    assert!(preview.contains("type=\"button\" aria-disabled=\"true\""));
    assert!(!preview.contains("/submissions?key="));
}

#[test]
fn published_forms_render_native_accessible_posts_and_escape_all_author_copy() {
    use eez_render::document_v2::render_published_route;

    let mut form_value: serde_json::Value = serde_json::from_str(
        std::str::from_utf8(document().canonical_bytes()).expect("canonical JSON"),
    )
    .expect("document JSON");
    form_value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"form-block", "kind":"form", "form_id":"form-one"}
    ]);
    form_value["forms"] = serde_json::json!([{
        "id":"form-one",
        "fields":[
            {"id":"field-email", "name":"email", "label":"Email & <work>", "help":"We'll only use this for the request.", "kind":"email", "required":true},
            {"id":"field-topic", "name":"topic", "label":"Topic", "kind":"select", "required":false, "options":["Demo & pricing"]}
        ],
        "purpose":"Request <information>",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to <be contacted>."},
        "marketing_opt_in":{"copy_version":"marketing-v2", "copy":"Send me news & offers."},
        "result":{"kind":"inline", "message":"Thanks & goodbye."}
    }]);
    let form_json = serde_json::to_vec(&form_value).expect("document serialization");
    let form_document = DocumentV2::decode(&form_json, "funnel-01").expect("form node schema");
    let prepared = prepare_revision(&form_document, "revision-form", &ResolvedV2Assets::empty())
        .expect("form revision");

    let html = render_published_route(
        2,
        "funnel-01",
        "revision-form",
        &prepared.data_json,
        &prepared.render_manifest_json,
        "lead-funnel",
        None,
    )
    .expect("published form page renders");

    assert!(html.contains("method=\"post\" enctype=\"application/x-www-form-urlencoded\""));
    assert!(html.contains("/api/public/funnels/lead-funnel/pages/page-home/forms/form-one/submissions?key=EEZ_FORM_SUBMISSION_KEY"));
    assert!(html.contains("type=\"email\""));
    assert!(html.contains("id=\"form-form-one-field-field-email\" name=\"email\""));
    assert!(html.contains("aria-describedby=\"form-form-one-field-field-email-help\""));
    assert!(html.contains("name=\"_request_consent\" value=\"on\" required>"));
    assert!(html.contains("name=\"_marketing_opt_in\" value=\"on\">"));
    assert!(html.contains("required"));
    assert!(html.contains("Email &amp; &lt;work&gt;"));
    assert!(html.contains("I agree to &lt;be contacted&gt;."));
    assert!(html.contains("Send me news &amp; offers."));
    assert!(html.contains("id=\"form-success-form-one\" role=\"status\""));
    assert!(html.contains("Thanks &amp; goodbye."));
    assert!(!html.contains("<work>"));
}

#[test]
fn v2_public_pages_render_and_internal_edges_use_canonical_routes() {
    use eez_render::document_v2::render_published_route;

    let mut value: serde_json::Value = serde_json::from_str(
        std::str::from_utf8(document().canonical_bytes()).expect("canonical JSON"),
    )
    .expect("document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-next", "kind":"button", "label":"Next", "action":{"kind":"navigate_page", "page_id":"page-next"}}
    ]);
    value["pages"]
        .as_array_mut()
        .expect("pages")
        .push(serde_json::json!({
            "id":"page-next",
            "slug":"next",
            "title":"Next page",
            "sections":[{
                "id":"section-next",
                "type":"hero",
                "blocks":[{
                    "id":"block-next-title",
                    "kind":"heading",
                    "level":1,
                    "text":"Next page content"
                }]
            }]
        }));
    let data_json = serde_json::to_vec(&value).expect("document serialization");
    let multipage = DocumentV2::decode(&data_json, "funnel-01").expect("valid page graph");
    let prepared = prepare_revision(&multipage, "revision-multipage", &ResolvedV2Assets::empty())
        .expect("manifest");

    let entry = render_published_route(
        2,
        "funnel-01",
        "revision-multipage",
        &prepared.data_json,
        &prepared.render_manifest_json,
        "quick-offer",
        None,
    )
    .expect("entry page renders");
    assert!(entry.contains("<a class=\"v2-button\" href=\"/f/quick-offer/next\">Next</a>"));

    let next = render_published_route(
        2,
        "funnel-01",
        "revision-multipage",
        &prepared.data_json,
        &prepared.render_manifest_json,
        "quick-offer",
        Some("next"),
    )
    .expect("requested page renders");
    assert!(next.contains("<title>Next page</title>"));
    assert!(next.contains("Next page content"));
    assert_ne!(entry, next);
    assert_eq!(
        render_published_route(
            2,
            "funnel-01",
            "revision-multipage",
            &prepared.data_json,
            &prepared.render_manifest_json,
            "quick-offer",
            Some("missing"),
        )
        .expect_err("unknown page is not rendered"),
        V2RenderError::PageMissing
    );
}

struct TestIds(u32);

impl eez_domain::document_v2::IdSource for TestIds {
    fn next_id(
        &mut self,
        _kind: eez_domain::document_v2::IdKind,
    ) -> Result<String, eez_domain::document_v2::DocumentError> {
        self.0 += 1;
        Ok(format!("test-{}", self.0))
    }
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
