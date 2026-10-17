use eez_domain::document_v2::{BlueprintId, DocumentV2};
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
fn exact_renderer_v3_manifest_binds_row_document_theme_and_assets() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-v2-01").expect("revision preparation");

    assert_eq!(prepared.manifest.renderer_version(), "renderer-v3");
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
fn page_theme_background_spans_full_width_with_bounded_inner_content() {
    let document = DocumentV2::starter(
        "funnel-01",
        BlueprintId::LeadGen,
        eez_domain::ThemePreset::Bold,
    )
    .expect("valid V2 starter");
    let html = render_owner_preview(&document, document.entry_page_id()).expect("owner preview");

    assert!(html.contains("<main id=\"funnel\"><div class=\"v2-main\">"));
    assert!(html.contains(".v2-main{max-width:var(--content-width);margin:0 auto}"));
}

#[test]
fn owner_read_validates_the_stored_v2_row_and_manifest_without_public_render_gates() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-owner-read").expect("manifest");
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
    let prepared = prepare_revision(&document, "revision-v2-01").expect("revision preparation");
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
        .replace("renderer-v3", "renderer-v3-next");
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
    let html = render_owner_preview(&edited.document, &page_id).expect("owner preview");
    assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!html.contains("<img src=x"));
}

#[test]
fn v2_text_blocks_preserve_line_breaks_and_tabs() {
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("canonical document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-copy", "kind":"text", "text":"First line\nSecond\tcolumn"}
    ]);
    let encoded = serde_json::to_vec(&value).expect("document serialization");
    let document = DocumentV2::decode(&encoded, "funnel-01").expect("multiline text block");
    let html = render_owner_preview(&document, document.entry_page_id()).expect("owner preview");

    assert!(html.contains("<p class=\"v2-text\">First line\nSecond\tcolumn</p>"));
    assert!(!html.contains("&#xfffd;"));
}

#[test]
fn v2_image_and_form_content_are_not_saved_or_rendered_before_their_gates() {
    let mut image_value: serde_json::Value = serde_json::from_str(
        std::str::from_utf8(document().canonical_bytes()).expect("canonical JSON"),
    )
    .expect("document JSON");
    image_value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"image-block", "kind":"image", "asset_id":"asset-one", "alt":{"kind":"decorative"}}
    ]);
    let image_json = serde_json::to_vec(&image_value).expect("document serialization");
    let image_document = DocumentV2::decode(&image_json, "funnel-01").expect("image node schema");
    assert!(matches!(
        prepare_revision(&image_document, "revision-image"),
        Err(V2RenderError::ImagesNotEnabled)
    ));

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
    assert!(matches!(
        render_owner_preview(&form_document, form_document.entry_page_id()),
        Err(V2RenderError::FormsNotEnabled)
    ));
}

#[test]
fn v2_internal_page_edges_do_not_claim_public_routes_before_pr027() {
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
        .push(serde_json::json!({"id":"page-next", "slug":"next", "title":"Next", "sections":[]}));
    let data_json = serde_json::to_vec(&value).expect("document serialization");
    let multipage = DocumentV2::decode(&data_json, "funnel-01").expect("valid page graph");
    let prepared = prepare_revision(&multipage, "revision-multipage").expect("manifest");

    assert!(matches!(
        render_published(
            2,
            "funnel-01",
            "revision-multipage",
            &prepared.data_json,
            &prepared.render_manifest_json,
        ),
        Err(V2RenderError::MultiPagePublicRouteUnavailable)
    ));
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
