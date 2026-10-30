use super::model::{
    BlockId, BlockV2, BlueprintId, ConsentCopy, DocumentError, FieldId, FormField, FormId,
    FormPresentation, FormResult, FormV2, PageId, PageV2, PublicLanguage, SectionId, SectionKind,
    SectionV2, StableDocumentId, TextPresentation,
};
use super::{GeneratedPlaceholderCode, GeneratedPlaceholderMarker, GeneratedPlaceholderTarget};

pub(super) fn starter_section(
    section_type: SectionKind,
    blueprint_id: BlueprintId,
    language: PublicLanguage,
) -> Result<SectionV2, DocumentError> {
    let (section_id, blocks) = match section_type {
        SectionKind::Hero if blueprint_id == BlueprintId::LeadGen => (
            "section-hero",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-hero-title")?,
                    level: 1,
                    text: copy(language, "A clear service offer", "Une offre de service claire"),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-hero-copy")?,
                    text: copy(
                        language,
                        "Describe who you help and what your service includes.",
                        "Décrivez les personnes que vous accompagnez et le contenu de votre service.",
                    ),
                },
            ],
        ),
        SectionKind::Hero => (
            "section-hero",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-hero-title")?,
                    level: 1,
                    text: copy(
                        language,
                        "Welcome to your new funnel",
                        "Bienvenue dans votre nouvel entonnoir",
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-hero-copy")?,
                    text: copy(
                        language,
                        "Replace this text with your offer.",
                        "Remplacez ce texte par votre offre.",
                    ),
                },
            ],
        ),
        SectionKind::Features if blueprint_id == BlueprintId::LeadGen => (
            "section-features",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-features-title")?,
                    level: 2,
                    text: copy(language, "What the service includes", "Ce que comprend le service"),
                },
                BlockV2::List {
                    id: BlockId::parse("block-features-list")?,
                    items: vec![
                        copy(
                            language,
                            "Describe the work included in the service.",
                            "Décrivez les prestations incluses.",
                        ),
                        copy(
                            language,
                            "Add an important limit or condition.",
                            "Indiquez une limite ou une condition importante.",
                        ),
                    ],
                },
            ],
        ),
        SectionKind::Features => (
            "section-features",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-features-title")?,
                    level: 2,
                    text: copy(
                        language,
                        "What makes this offer valuable",
                        "Les avantages de cette offre",
                    ),
                },
                BlockV2::List {
                    id: BlockId::parse("block-features-list")?,
                    items: vec![
                        copy(
                            language,
                            "A clear outcome for your customers",
                            "Un résultat clair pour vos clients",
                        ),
                        copy(
                            language,
                            "A simple next step to take action",
                            "Une prochaine étape simple",
                        ),
                    ],
                },
            ],
        ),
        SectionKind::Testimonials => (
            "section-testimonials",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-testimonials-title")?,
                    level: 2,
                    text: copy(language, "Customer stories", "Témoignages clients"),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-testimonials-copy")?,
                    text: copy(
                        language,
                        "Add a real customer story and attribution here, or remove this section.",
                        "Ajoutez ici un témoignage client réel et son attribution, ou supprimez cette section.",
                    ),
                },
            ],
        ),
        SectionKind::Cta if blueprint_id == BlueprintId::LeadGen => (
            "section-cta",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-cta-title")?,
                    level: 2,
                    text: copy(language, "Tell us about your project", "Parlez-nous de votre besoin"),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-cta-copy")?,
                    text: copy(
                        language,
                        "Share a few details so the service provider can review your request.",
                        "Partagez quelques détails pour permettre au prestataire d’étudier votre demande.",
                    ),
                },
                BlockV2::Form {
                    id: BlockId::parse("block-service-inquiry-form")?,
                    form_id: FormId::parse("form-service-inquiry")?,
                },
            ],
        ),
        SectionKind::Cta => (
            "section-cta",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-cta-title")?,
                    level: 2,
                    text: copy(language, "Add a next step", "Ajoutez une prochaine étape"),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-cta-copy")?,
                    text: copy(
                        language,
                        "Choose a real destination before adding a call to action.",
                        "Choisissez une destination réelle avant d’ajouter un appel à l’action.",
                    ),
                },
            ],
        ),
        SectionKind::Footer => (
            "section-footer",
            vec![BlockV2::Text {
                id: BlockId::parse("block-footer-copy")?,
                text: copy(
                    language,
                    "Your business · All rights reserved.",
                    "Votre entreprise · Tous droits réservés.",
                ),
            }],
        ),
    };
    Ok(SectionV2 {
        id: SectionId::parse(section_id.to_owned())?,
        section_type,
        blocks,
    })
}

pub(super) fn generated_placeholders(
    blueprint_id: BlueprintId,
    language: PublicLanguage,
) -> Result<Vec<GeneratedPlaceholderMarker>, DocumentError> {
    let mut markers = Vec::new();
    if blueprint_id != BlueprintId::LeadGen {
        markers.push(GeneratedPlaceholderMarker {
            target: GeneratedPlaceholderTarget::Block {
                page_id: PageId::parse("page-home")?,
                section_id: SectionId::parse("section-hero")?,
                block_id: BlockId::parse("block-hero-copy")?,
            },
            code: GeneratedPlaceholderCode::OfferText,
            expected_text: copy(
                language,
                "Replace this text with your offer.",
                "Remplacez ce texte par votre offre.",
            ),
        });
    }
    if matches!(
        blueprint_id,
        BlueprintId::Vsl | BlueprintId::Webinar | BlueprintId::Launch
    ) {
        markers.push(GeneratedPlaceholderMarker {
            target: GeneratedPlaceholderTarget::Block {
                page_id: PageId::parse("page-home")?,
                section_id: SectionId::parse("section-testimonials")?,
                block_id: BlockId::parse("block-testimonials-copy")?,
            },
            code: GeneratedPlaceholderCode::TestimonialPrompt,
            expected_text: copy(
                language,
                "Add a real customer story and attribution here, or remove this section.",
                "Ajoutez ici un témoignage client réel et son attribution, ou supprimez cette section.",
            ),
        });
    }
    Ok(markers)
}

pub(super) fn service_request_form(language: PublicLanguage) -> Result<FormV2, DocumentError> {
    let success_page = PageId::parse("page-request-received")?;
    let (consent_version, consent_copy) = match language {
        PublicLanguage::English => (
            "request-ack-service-en",
            "I have read and understood the information about this request.",
        ),
        PublicLanguage::French => (
            "request-ack-service-fr",
            "J’ai pris connaissance des informations relatives à cette demande.",
        ),
    };

    Ok(FormV2 {
        id: FormId::parse("form-service-inquiry")?,
        fields: vec![
            FormField::Text {
                id: FieldId::parse("field-name")?,
                name: "name".to_owned(),
                label: copy(language, "Your name", "Votre nom"),
                help: None,
                required: false,
                validation: None,
                presentation: TextPresentation::SingleLine,
            },
            FormField::Email {
                id: FieldId::parse("field-email")?,
                name: "email".to_owned(),
                label: copy(language, "Email address", "Adresse e-mail"),
                help: None,
                required: true,
                validation: None,
            },
            FormField::Text {
                id: FieldId::parse("field-project-details")?,
                name: "project_details".to_owned(),
                label: copy(language, "Describe your project", "Décrivez votre besoin"),
                help: Some(copy(
                    language,
                    "Share the service you need and any useful context.",
                    "Précisez le service recherché et tout élément utile.",
                )),
                required: true,
                validation: None,
                presentation: TextPresentation::Multiline,
            },
        ],
        purpose: copy(language, "Service inquiry", "Demande de service"),
        request_consent: ConsentCopy {
            copy_version: StableDocumentId::parse(consent_version.to_owned())?,
            copy: consent_copy.to_owned(),
        },
        marketing_opt_in: None,
        result: FormResult::SuccessPage {
            page_id: success_page,
        },
        presentation: Some(FormPresentation {
            submit_label: Some(copy(language, "Send request", "Envoyer la demande")),
            information_url: None,
        }),
    })
}

pub(super) fn service_request_confirmation(
    language: PublicLanguage,
) -> Result<PageV2, DocumentError> {
    Ok(PageV2 {
        id: PageId::parse("page-request-received")?,
        slug: "request-received".to_owned(),
        title: copy(language, "Request received", "Demande reçue"),
        seo_title: None,
        seo_description: None,
        noindex: true,
        sections: vec![SectionV2 {
            id: SectionId::parse("section-request-received")?,
            section_type: SectionKind::Hero,
            blocks: vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-request-received-title")?,
                    level: 1,
                    text: copy(
                        language,
                        "Your request has been received.",
                        "Votre demande a bien été reçue.",
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-request-received-follow-up")?,
                    text: copy(
                        language,
                        "The service provider can review your request and contact you about next steps.",
                        "Le prestataire pourra étudier votre demande et vous recontacter au sujet de la suite.",
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-request-received-boundary")?,
                    text: copy(
                        language,
                        "This confirms receipt only; it is not a reservation or a confirmed service.",
                        "Ce message confirme seulement la réception de votre demande ; il ne s’agit ni d’une réservation ni d’une prestation confirmée.",
                    ),
                },
            ],
        }],
    })
}

fn copy(language: PublicLanguage, english: &str, french: &str) -> String {
    match language {
        PublicLanguage::English => english,
        PublicLanguage::French => french,
    }
    .to_owned()
}
