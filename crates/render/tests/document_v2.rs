use eez_domain::document_v2::{
    BlueprintId, DocumentV2, FormId, FormSubmissionIssue, FormSubmissionIssueCode,
    FormSubmissionIssueTarget, PublicLanguage,
};
use eez_render::document_v2::{
    prepare_revision, prepare_revision_with_assets, render_owner_preview,
    render_owner_preview_with_assets, render_published, render_published_route_with_assets,
    render_published_route_with_submission_feedback, validate_revision_v2, AssetDigestRecord,
    FormErrorPageKind, FormRecoveryNotice, FormSubmissionFeedback, PublishedRevisionInput,
    V2RenderError,
};

fn document() -> DocumentV2 {
    DocumentV2::starter(
        "funnel-01",
        BlueprintId::LeadGen,
        eez_domain::ThemePreset::Minimal,
    )
    .expect("valid V2 starter")
}

fn manifest_with_renderer_version(manifest_json: &str, version: &str) -> String {
    let mut manifest: serde_json::Value =
        serde_json::from_str(manifest_json).expect("serialized manifest");
    manifest["renderer_version"] = serde_json::Value::String(version.to_owned());
    String::from_utf8(serde_json_canonicalizer::to_vec(&manifest).expect("canonical manifest"))
        .expect("UTF-8 manifest")
}

#[test]
fn exact_renderer_v8_manifest_binds_row_document_theme_and_assets() {
    let document = DocumentV2::starter(
        "funnel-01",
        BlueprintId::Launch,
        eez_domain::ThemePreset::Minimal,
    )
    .expect("valid V2 starter");
    let prepared = prepare_revision(&document, "revision-v2-01").expect("revision preparation");

    assert_eq!(prepared.manifest.renderer_version(), "renderer-v8");
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
    assert!(html.contains("--shadow-card:0 1px 3px rgba(15, 23, 42, 0.08)"));
    assert!(html.contains(
        ".v2-section--cta{text-align:center;background:var(--color-surface);border-radius:var(--radius-card);box-shadow:var(--shadow-card)}"
    ));
    for section in ["hero", "features", "testimonials", "cta", "footer"] {
        assert!(html.contains(&format!("class=\"v2-section v2-section--{section}\"")));
    }
    assert!(html.contains("<h1>Welcome to your new funnel</h1>"));
    assert!(html.contains(
        "<ul class=\"v2-list\"><li>A clear outcome for your customers</li><li>A simple next step to take action</li></ul>"
    ));
    assert!(html.contains(
        "<a class=\"v2-button\" href=\"https://example.com/\" target=\"_blank\" rel=\"noopener noreferrer\">Get started</a>"
    ));
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
    assert!(!html.contains("data-eezfunnel-page-id"));
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
        .replace("renderer-v8", "renderer-v8-next");
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
    assert!(html.contains("<span class=\"v2-button\" aria-disabled=\"true\">Get started</span>"));
    assert!(!html.contains("<a class=\"v2-button\""));
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
fn v2_images_remain_blocked_and_form_previews_are_inert_until_published() {
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
    let preview = render_owner_preview(&form_document, form_document.entry_page_id())
        .expect("owner preview renders a form without a live submission target");
    assert!(preview.contains("data-preview=\"true\""));
    assert!(preview.contains("type=\"button\" aria-disabled=\"true\""));
    assert!(!preview.contains("/submissions?key="));
}

#[test]
fn v2_images_render_only_with_a_complete_digest_bound_asset_manifest() {
    let asset_id = "a".repeat(64);
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("canonical document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {
            "id":"image-block",
            "kind":"image",
            "asset_id":asset_id,
            "alt":{"kind":"text", "value":"An image & <description>"}
        }
    ]);
    let encoded = serde_json::to_vec(&value).expect("document serialization");
    let image_document = DocumentV2::decode(&encoded, "funnel-01").expect("image document");
    let assets = vec![AssetDigestRecord {
        asset_id: asset_id.clone(),
        sha256: asset_id.clone(),
    }];
    let prepared = prepare_revision_with_assets(&image_document, "revision-image", &assets)
        .expect("asset-backed revision");
    let preview =
        render_owner_preview_with_assets(&image_document, image_document.entry_page_id(), &assets)
            .expect("asset-backed owner preview");

    assert!(preview.contains(&format!("src=\"/media/{asset_id}\"")));
    assert!(preview.contains("alt=\"An image &amp; &lt;description&gt;\""));
    assert!(preview.contains("loading=\"lazy\" decoding=\"async\""));
    assert!(render_published_route_with_assets(
        PublishedRevisionInput {
            schema_version: 2,
            funnel_id: "funnel-01",
            revision_id: "revision-image",
            data_json: &prepared.data_json,
            render_manifest_json: &prepared.render_manifest_json,
        },
        "lead-funnel",
        None,
        &assets,
    )
    .expect("asset-backed published page")
    .contains(&format!("src=\"/media/{asset_id}\"")));

    assert_eq!(
        prepare_revision_with_assets(&image_document, "revision-no-asset", &[])
            .expect_err("missing assets fail closed"),
        V2RenderError::ImagesNotEnabled
    );
    assert_eq!(
        render_owner_preview_with_assets(
            &image_document,
            image_document.entry_page_id(),
            &[AssetDigestRecord {
                asset_id: asset_id.clone(),
                sha256: "b".repeat(64),
            }],
        )
        .expect_err("mismatched immutable identity is rejected"),
        V2RenderError::InvalidManifest
    );
    assert_eq!(
        render_published_route_with_assets(
            PublishedRevisionInput {
                schema_version: 2,
                funnel_id: "funnel-01",
                revision_id: "revision-image",
                data_json: &prepared.data_json,
                render_manifest_json: &prepared.render_manifest_json,
            },
            "lead-funnel",
            None,
            &[AssetDigestRecord {
                asset_id: asset_id.clone(),
                sha256: "b".repeat(64),
            }],
        )
        .expect_err("a changed asset cannot satisfy the saved manifest"),
        V2RenderError::InvalidManifest
    );
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
    let prepared = prepare_revision(&form_document, "revision-form").expect("form revision");
    let renderer_v4_manifest =
        manifest_with_renderer_version(&prepared.render_manifest_json, "renderer-v4");

    assert_eq!(
        render_published_route(
            2,
            "funnel-01",
            "revision-form",
            &prepared.data_json,
            &renderer_v4_manifest,
            "lead-funnel",
            None,
        )
        .expect_err("renderer-v4 did not support forms"),
        V2RenderError::FormsNotEnabled
    );

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
    assert!(html.contains("<main id=\"funnel\" data-eezfunnel-page-id=\"page-home\">"));
    assert!(html.contains("<form class=\"v2-form\" data-eezfunnel-form-id=\"form-one\""));
    assert!(html.contains("/api/public/funnels/lead-funnel/pages/page-home/forms/form-one/submissions?key=EEZ_FORM_SUBMISSION_KEY"));
    assert!(html.contains("type=\"email\""));
    assert!(html.contains("id=\"form-form-one-field-field-email\" name=\"email\""));
    assert!(html.contains("aria-describedby=\"form-form-one-field-field-email-help\""));
    assert!(html.contains("name=\"_request_consent\" value=\"on\" required>"));
    assert!(html.contains("name=\"_marketing_opt_in\" value=\"on\">"));
    assert!(!html.contains(".v2-form-errors"));
    assert!(!html.contains("<option value=\"\" selected>Choose an option</option>"));
    assert!(html.contains("required"));
    assert!(html.contains("Email &amp; &lt;work&gt;"));
    assert!(html.contains("I agree to &lt;be contacted&gt;."));
    assert!(html.contains("Send me news &amp; offers."));
    assert!(html.contains("id=\"form-success-form-one\" role=\"status\""));
    assert!(html.contains("Thanks &amp; goodbye."));
    assert!(!html.contains("<work>"));
}

#[test]
fn renderer_v8_localizes_forms_and_keeps_multiline_fields_and_information_links_inert_in_preview() {
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("document JSON");
    value["language"] = serde_json::json!("fr");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"form-block", "kind":"form", "form_id":"form-one"}
    ]);
    value["forms"] = serde_json::json!([{
        "id":"form-one",
        "fields":[
            {"id":"field-name", "name":"name", "label":"Votre nom", "kind":"text", "required":true, "presentation":"multiline"},
            {"id":"field-email", "name":"email", "label":"Adresse e-mail", "kind":"email", "required":true}
        ],
        "purpose":"Demande de contact",
        "request_consent":{"copy_version":"request-v1", "copy":"J’accepte d’être contacté."},
        "result":{"kind":"inline", "message":"Merci."},
        "presentation":{"submit_label":"Envoyer ma demande", "information_url":"https://example.test/privacy"}
    }]);
    let document = DocumentV2::decode(
        &serde_json::to_vec(&value).expect("French JSON"),
        "funnel-01",
    )
    .expect("French form document");
    let prepared = prepare_revision(&document, "revision-fr-form").expect("French revision");
    let published = render_published_route_with_assets(
        PublishedRevisionInput {
            schema_version: 2,
            funnel_id: "funnel-01",
            revision_id: "revision-fr-form",
            data_json: &prepared.data_json,
            render_manifest_json: &prepared.render_manifest_json,
        },
        "lead-funnel",
        None,
        &[],
    )
    .expect("published French form");
    let preview = render_owner_preview(&document, document.entry_page_id()).expect("preview");

    assert_eq!(document.public_language(), PublicLanguage::French);
    assert!(published.contains("<html lang=\"fr\">"));
    assert!(published.contains("<textarea id=\"form-form-one-field-field-name\" name=\"name\" rows=\"4\" maxlength=\"8192\" required"));
    assert!(published.contains("Obligatoire"));
    assert!(published.contains("Envoyer ma demande"));
    assert!(published.contains("<a href=\"https://example.test/privacy\""));
    assert!(published.contains("Informations sur cette demande"));
    assert!(preview.contains("<span aria-disabled=\"true\">Informations sur cette demande</span>"));
    assert!(!preview.contains("<a href=\"https://example.test/privacy\""));

    let form_id = FormId::parse("form-one").expect("form id");
    let issues = [FormSubmissionIssue {
        target: FormSubmissionIssueTarget::Field("field-name".to_owned()),
        code: FormSubmissionIssueCode::Required,
    }];
    let values =
        std::collections::BTreeMap::from([("name".to_owned(), "\nGrace\nHopper".to_owned())]);
    let feedback = FormSubmissionFeedback {
        form_id: &form_id,
        issues: &issues,
        values: &values,
        request_consent_accepted: true,
        marketing_opt_in: false,
        notice: Some(FormRecoveryNotice::SubmissionUncertain),
    };
    let recovery = eez_render::document_v2::render_published_route_with_submission_feedback(
        PublishedRevisionInput {
            schema_version: 2,
            funnel_id: "funnel-01",
            revision_id: "revision-fr-form",
            data_json: &prepared.data_json,
            render_manifest_json: &prepared.render_manifest_json,
        },
        "lead-funnel",
        None,
        &[],
        &feedback,
    )
    .expect("French recovery page");
    assert!(recovery.contains("Ce formulaire contient une erreur"));
    assert!(recovery.contains("Ce champ est obligatoire."));
    assert!(recovery.contains("Nous n’avons pas pu confirmer votre demande."));
    assert!(recovery.contains(">\n\nGrace\nHopper</textarea>"));
}

#[test]
fn stale_revision_and_key_reuse_pages_use_the_trusted_document_language() {
    let stale = eez_render::document_v2::render_form_error_page(
        PublicLanguage::French,
        FormErrorPageKind::StaleRevision,
        Some("/f/service"),
    );
    assert!(stale.contains("<html lang=\"fr\">"));
    assert!(stale.contains("Ce formulaire a été modifié"));
    assert!(stale.contains("Rechargez la page actuelle"));
    assert!(stale.contains("href=\"/f/service\">Ouvrir le formulaire actuel</a>"));

    let conflict = eez_render::document_v2::render_form_error_page(
        PublicLanguage::English,
        FormErrorPageKind::KeyReuseMismatch,
        Some("/f/service?x=\"bad\""),
    );
    assert!(conflict.contains("<html lang=\"en\">"));
    assert!(conflict.contains("Form response already used"));
    assert!(conflict.contains("href=\"/f/service?x=&quot;bad&quot;\""));
}

#[test]
fn form_validation_feedback_links_errors_and_preserves_only_safe_known_values() {
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
            {"id":"field-email", "name":"email", "label":"Email", "help":"Use a work address.", "kind":"email", "required":true},
            {"id":"field-topic", "name":"topic", "label":"Topic", "kind":"select", "required":true, "options":["Demo", "Pricing"]}
        ],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
        "marketing_opt_in":{"copy_version":"marketing-v1", "copy":"Send me product updates."},
        "result":{"kind":"inline", "message":"Thanks."}
    }]);
    let form_document = DocumentV2::decode(
        &serde_json::to_vec(&form_value).expect("document serialization"),
        "funnel-01",
    )
    .expect("form document");
    let prepared = prepare_revision(&form_document, "revision-feedback").expect("revision");
    let form_id = FormId::parse("form-one").expect("form id");
    let issues = [
        FormSubmissionIssue {
            target: FormSubmissionIssueTarget::Field("field-email".to_owned()),
            code: FormSubmissionIssueCode::InvalidValue,
        },
        FormSubmissionIssue {
            target: FormSubmissionIssueTarget::Field("field-topic".to_owned()),
            code: FormSubmissionIssueCode::InvalidChoice,
        },
        FormSubmissionIssue {
            target: FormSubmissionIssueTarget::RequestConsent,
            code: FormSubmissionIssueCode::RequestConsentRequired,
        },
    ];
    let values = std::collections::BTreeMap::from([(
        "email".to_owned(),
        "<ada>@example.test\" autofocus=\"true".to_owned(),
    )]);
    let feedback = FormSubmissionFeedback {
        form_id: &form_id,
        issues: &issues,
        values: &values,
        request_consent_accepted: false,
        marketing_opt_in: false,
        notice: None,
    };

    let html = render_published_route_with_submission_feedback(
        PublishedRevisionInput {
            schema_version: 2,
            funnel_id: "funnel-01",
            revision_id: "revision-feedback",
            data_json: &prepared.data_json,
            render_manifest_json: &prepared.render_manifest_json,
        },
        "lead-funnel",
        None,
        &[],
        &feedback,
    )
    .expect("recovery form renders");

    assert!(html.contains("<meta name=\"robots\" content=\"noindex\">"));
    assert!(html.contains("role=\"alert\""));
    assert!(html.contains("href=\"#form-form-one-field-field-email\""));
    assert!(html.contains("href=\"#form-form-one-field-field-topic\""));
    assert!(html.contains("href=\"#form-form-one-request-consent\""));
    assert!(html.contains("aria-invalid=\"true\" aria-describedby=\"form-form-one-field-field-email-help form-form-one-field-field-email-error\""));
    assert!(html.contains("value=\"&lt;ada&gt;@example.test&quot; autofocus=&quot;true\""));
    assert!(html.contains("id=\"form-form-one-field-field-email-error\""));
    assert!(html.contains("<option value=\"\" selected>Choose an option</option>"));
    assert!(!html.contains("<ada>"));
    assert!(!html.contains("value=\"<script>\""));
    assert!(!html.contains("name=\"_request_consent\" value=\"on\" required checked"));
    assert!(!html.contains("name=\"_marketing_opt_in\" value=\"on\" checked"));
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
    let prepared = prepare_revision(&multipage, "revision-multipage").expect("manifest");
    let renderer_v4_manifest =
        manifest_with_renderer_version(&prepared.render_manifest_json, "renderer-v4");
    let legacy_manifest =
        manifest_with_renderer_version(&prepared.render_manifest_json, "renderer-v3");

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

    let renderer_v4_entry = render_published_route(
        2,
        "funnel-01",
        "revision-multipage",
        &prepared.data_json,
        &renderer_v4_manifest,
        "quick-offer",
        None,
    )
    .expect("stored V4 multipage revision keeps its routes");
    assert!(
        renderer_v4_entry.contains("<a class=\"v2-button\" href=\"/f/quick-offer/next\">Next</a>")
    );

    let legacy_entry = render_published_route(
        2,
        "funnel-01",
        "revision-multipage",
        &prepared.data_json,
        &legacy_manifest,
        "quick-offer",
        None,
    )
    .expect("stored V3 entry page remains readable");
    assert!(legacy_entry.contains("<span class=\"v2-button\" aria-disabled=\"true\">Next</span>"));
    assert!(!legacy_entry.contains("href=\"/f/quick-offer/next\""));
    assert_eq!(
        render_published_route(
            2,
            "funnel-01",
            "revision-multipage",
            &prepared.data_json,
            &legacy_manifest,
            "quick-offer",
            Some("next"),
        )
        .expect_err("V3 manifests cannot expose routes added by V4"),
        V2RenderError::PageMissing
    );

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

#[test]
fn renderer_v8_metadata_escapes_title_and_attributes_in_preview_and_public_html() {
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("document JSON");
    value["pages"][0]["seo_title"] = serde_json::json!("Offer <sale> & \"launch\"");
    value["pages"][0]["seo_description"] = serde_json::json!("A \"quote\" & <tag> 'offer'");
    value["pages"][0]["noindex"] = serde_json::json!(true);
    let document = DocumentV2::decode(&serde_json::to_vec(&value).expect("JSON"), "funnel-01")
        .expect("metadata document");
    let prepared = prepare_revision(&document, "revision-seo").expect("metadata revision");
    assert_eq!(prepared.manifest.renderer_version(), "renderer-v8");
    let published = render_published(
        2,
        "funnel-01",
        "revision-seo",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .expect("public metadata");
    let preview = render_owner_preview(&document, document.entry_page_id()).expect("preview");

    for html in [&published, &preview] {
        assert!(html.contains("<title>Offer &lt;sale&gt; &amp; \"launch\"</title>"));
        assert!(html.contains("<meta property=\"og:type\" content=\"website\">"));
        assert!(html.contains(
            "<meta property=\"og:title\" content=\"Offer &lt;sale&gt; &amp; &quot;launch&quot;\">"
        ));
        assert!(html.contains(
            "<meta name=\"description\" content=\"A &quot;quote&quot; &amp; &lt;tag&gt; &#39;offer&#39;\">"
        ));
        assert!(html.contains(
            "<meta property=\"og:description\" content=\"A &quot;quote&quot; &amp; &lt;tag&gt; &#39;offer&#39;\">"
        ));
        assert!(html.contains("<meta name=\"robots\" content=\"noindex\">"));
        assert!(!html.contains("<sale>"));
        assert!(!html.contains("rel=\"canonical\""));
        assert!(!html.contains("og:image"));
    }
    let changed_data = prepared.data_json.replace("&", "and");
    assert_eq!(
        render_published(
            2,
            "funnel-01",
            "revision-seo",
            &changed_data,
            &prepared.render_manifest_json,
        )
        .expect_err("metadata digest is bound"),
        V2RenderError::DigestMismatch
    );
}

#[test]
fn renderer_v8_falls_back_to_page_title_and_omits_unset_description_and_robots() {
    let document = document();
    let prepared = prepare_revision(&document, "revision-default-seo").expect("revision");
    let html = render_published(
        2,
        "funnel-01",
        "revision-default-seo",
        &prepared.data_json,
        &prepared.render_manifest_json,
    )
    .expect("default metadata");

    assert!(html.contains("<title>Home</title>"));
    assert!(html.contains("<meta property=\"og:title\" content=\"Home\">"));
    assert!(!html.contains("name=\"description\""));
    assert!(!html.contains("og:description"));
    assert!(!html.contains("name=\"robots\""));
}

#[test]
fn page_metadata_stays_on_its_page_and_is_rejected_before_renderer_v7() {
    use eez_render::document_v2::render_published_route;

    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"next-button","kind":"button","label":"Next","action":{"kind":"navigate_page","page_id":"page-next"}}
    ]);
    value["pages"]
        .as_array_mut()
        .expect("pages")
        .push(serde_json::json!({
            "id":"page-next", "slug":"next", "title":"Next", "sections":[]
        }));

    for (field, metadata) in [
        ("seo_title", serde_json::json!("Private offer title")),
        (
            "seo_description",
            serde_json::json!("Next page description"),
        ),
        ("noindex", serde_json::json!(true)),
    ] {
        let mut with_metadata = value.clone();
        with_metadata["pages"][1][field] = metadata;
        let document = DocumentV2::decode(
            &serde_json::to_vec(&with_metadata).expect("JSON"),
            "funnel-01",
        )
        .expect("two pages");
        let prepared = prepare_revision(&document, "revision-page-seo").expect("revision");
        let entry = render_published_route(
            2,
            "funnel-01",
            "revision-page-seo",
            &prepared.data_json,
            &prepared.render_manifest_json,
            "offer",
            None,
        )
        .expect("entry");
        let next = render_published_route(
            2,
            "funnel-01",
            "revision-page-seo",
            &prepared.data_json,
            &prepared.render_manifest_json,
            "offer",
            Some("next"),
        )
        .expect("next");

        assert!(entry.contains("<title>Home</title>"));
        assert!(!entry.contains("Private offer title"));
        assert!(!entry.contains("Next page description"));
        assert!(!entry.contains("name=\"robots\""));
        match field {
            "seo_title" => assert!(next.contains("<title>Private offer title</title>")),
            "seo_description" => assert!(
                next.contains("<meta name=\"description\" content=\"Next page description\">")
            ),
            _ => assert!(next.contains("<meta name=\"robots\" content=\"noindex\">")),
        }

        for renderer in ["renderer-v3", "renderer-v4", "renderer-v5", "renderer-v6"] {
            let manifest = manifest_with_renderer_version(&prepared.render_manifest_json, renderer);
            assert_eq!(
                render_published_route(
                    2,
                    "funnel-01",
                    "revision-page-seo",
                    &prepared.data_json,
                    &manifest,
                    "offer",
                    None,
                )
                .expect_err("legacy renderer rejects SEO anywhere in document"),
                V2RenderError::MetadataNotEnabled
            );
        }
    }
}

#[test]
fn renderer_v7_preserves_english_form_output_and_rejects_renderer_v8_features() {
    let mut value: serde_json::Value =
        serde_json::from_slice(document().canonical_bytes()).expect("document JSON");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"form-block", "kind":"form", "form_id":"form-one"}
    ]);
    value["forms"] = serde_json::json!([{
        "id":"form-one",
        "fields":[
            {"id":"field-name", "name":"name", "label":"Your name", "kind":"text", "required":true},
            {"id":"field-topic", "name":"topic", "label":"Topic", "kind":"select", "required":false, "options":["Demo"]}
        ],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
        "result":{"kind":"inline", "message":"Thanks."}
    }]);
    let base_document = DocumentV2::decode(
        &serde_json::to_vec(&value).expect("base form JSON"),
        "funnel-01",
    )
    .expect("base form document");
    let prepared = prepare_revision(&base_document, "revision-v7-form").expect("revision");
    let renderer_v7 = manifest_with_renderer_version(&prepared.render_manifest_json, "renderer-v7");
    let html = eez_render::document_v2::render_published_route(
        2,
        "funnel-01",
        "revision-v7-form",
        &prepared.data_json,
        &renderer_v7,
        "lead-funnel",
        None,
    )
    .expect("renderer v7 keeps its old form presentation");
    assert!(html.contains("<html lang=\"en\">"));
    assert!(
        html.contains("<input type=\"text\" id=\"form-form-one-field-field-name\" name=\"name\"")
    );
    assert!(html.contains("<button class=\"v2-button\" type=\"submit\">Send</button>"));
    assert!(!html.contains("<textarea"));
    assert!(!html.contains("v2-field-requirement"));
    assert!(!html.contains("<option value=\"\" selected>Choose an option</option>"));

    let mut localized = value.clone();
    localized["language"] = serde_json::json!("fr");
    let french_document = DocumentV2::decode(
        &serde_json::to_vec(&localized).expect("French document JSON"),
        "funnel-01",
    )
    .expect("French document remains a valid draft");
    let french_revision =
        prepare_revision(&french_document, "revision-v7-language").expect("French revision");
    assert_eq!(
        validate_revision_v2(
            2,
            "funnel-01",
            "revision-v7-language",
            &french_revision.data_json,
            &manifest_with_renderer_version(&french_revision.render_manifest_json, "renderer-v7",),
        )
        .expect_err("v7 cannot render explicit language metadata"),
        V2RenderError::PublicLanguageNotEnabled
    );

    for (feature, expected_error) in [
        ("presentation", V2RenderError::FormPresentationNotEnabled),
        ("multiline", V2RenderError::MultilineTextNotEnabled),
    ] {
        let mut feature_value = value.clone();
        match feature {
            "presentation" => {
                feature_value["forms"][0]["presentation"] = serde_json::json!({
                    "submit_label":"Send the request"
                });
            }
            _ => {
                feature_value["forms"][0]["fields"][0]["presentation"] =
                    serde_json::json!("multiline");
            }
        }
        let feature_document = DocumentV2::decode(
            &serde_json::to_vec(&feature_value).expect("feature document JSON"),
            "funnel-01",
        )
        .expect("v8 feature document remains a valid draft");
        let feature_revision = prepare_revision(&feature_document, "revision-v7-feature")
            .expect("revision preparation");
        let feature_manifest =
            manifest_with_renderer_version(&feature_revision.render_manifest_json, "renderer-v7");
        assert_eq!(
            validate_revision_v2(
                2,
                "funnel-01",
                "revision-v7-feature",
                &feature_revision.data_json,
                &feature_manifest,
            )
            .expect_err("v7 rejects v8-only form feature"),
            expected_error
        );
    }
}
