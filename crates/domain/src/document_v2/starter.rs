use super::model::{
    BlockId, BlockV2, BlueprintId, ConsentCopy, DocumentError, FieldId, FormField, FormId,
    FormPresentation, FormResult, FormV2, PageId, PageV2, PublicLanguage, SectionId, SectionKind,
    SectionV2, StableDocumentId, TextPresentation,
};
use super::{GeneratedPlaceholderCode, GeneratedPlaceholderMarker, GeneratedPlaceholderTarget};
use crate::ServiceRecipe;

pub(super) fn starter_section(
    section_type: SectionKind,
    blueprint_id: BlueprintId,
    language: PublicLanguage,
    service_recipe: Option<ServiceRecipe>,
) -> Result<SectionV2, DocumentError> {
    let (section_id, blocks) = match section_type {
        SectionKind::Hero if blueprint_id == BlueprintId::LeadGen => (
            "section-hero",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-hero-title")?,
                    level: 1,
                    text: service_copy(
                        language,
                        service_recipe,
                        ("A clear service offer", "Une offre de service claire"),
                        ("Request a scoped estimate", "Demandez un devis adapté au périmètre"),
                        ("Request a diagnostic discussion", "Demandez un échange de diagnostic"),
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-hero-copy")?,
                    text: service_copy(
                        language,
                        service_recipe,
                        (
                            "Describe who you help and what your service includes.",
                            "Décrivez les personnes que vous accompagnez et le contenu de votre service.",
                        ),
                        (
                            "Share the work you have in mind so the provider can review whether an estimate is suitable.",
                            "Décrivez les travaux envisagés pour permettre au prestataire d’étudier si un devis est adapté.",
                        ),
                        (
                            "Describe the topic you want to examine. This request does not schedule a session or provide a diagnosis.",
                            "Décrivez le sujet que vous souhaitez examiner. Cette demande ne réserve pas de séance et ne fournit pas de diagnostic.",
                        ),
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
                    text: service_copy(
                        language,
                        service_recipe,
                        ("Tell us about your project", "Parlez-nous de votre besoin"),
                        ("Describe the scope for your estimate", "Précisez le périmètre du devis"),
                        ("Tell us what you would like to examine", "Décrivez le point que vous souhaitez examiner"),
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-cta-copy")?,
                    text: service_copy(
                        language,
                        service_recipe,
                        (
                            "Share a few details so the service provider can review your request.",
                            "Partagez quelques détails pour permettre au prestataire d’étudier votre demande.",
                        ),
                        (
                            "Include the work you want priced and any timing context that matters to you.",
                            "Indiquez les prestations à chiffrer et, si utile, le calendrier envisagé.",
                        ),
                        (
                            "Share the situation and the question you want to clarify. No score or automatic assessment is produced.",
                            "Présentez la situation et la question à éclaircir. Aucun score ni diagnostic automatique n’est produit.",
                        ),
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

pub(super) fn service_request_form(
    language: PublicLanguage,
    service_recipe: Option<ServiceRecipe>,
) -> Result<FormV2, DocumentError> {
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

    let fields = match service_recipe {
        None | Some(ServiceRecipe::Contact) => vec![
            name_field(language)?,
            email_field(language)?,
            text_field(
                "field-project-details",
                "project_details",
                copy(language, "Describe your project", "Décrivez votre besoin"),
                Some(copy(
                    language,
                    "Share the service you need and any useful context.",
                    "Précisez le service recherché et tout élément utile.",
                )),
                true,
                TextPresentation::Multiline,
            )?,
        ],
        Some(ServiceRecipe::QuoteRequest) => vec![
            name_field(language)?,
            email_field(language)?,
            text_field(
                "field-quote-scope",
                "quote_scope",
                copy(
                    language,
                    "Describe the scope for an estimate",
                    "Décrivez le périmètre du devis",
                ),
                Some(copy(
                    language,
                    "List the work you would like included in an estimate.",
                    "Indiquez les prestations que vous souhaitez faire chiffrer.",
                )),
                true,
                TextPresentation::Multiline,
            )?,
            text_field(
                "field-quote-timeline",
                "quote_timeline",
                copy(
                    language,
                    "Preferred timeline (optional)",
                    "Calendrier envisagé (facultatif)",
                ),
                Some(copy(
                    language,
                    "Sharing a timeline does not confirm availability.",
                    "Indiquer un calendrier ne confirme pas la disponibilité.",
                )),
                false,
                TextPresentation::SingleLine,
            )?,
        ],
        Some(ServiceRecipe::DiagnosticApplication) => vec![
            name_field(language)?,
            email_field(language)?,
            text_field(
                "field-current-situation",
                "current_situation",
                copy(
                    language,
                    "Describe the current situation",
                    "Décrivez la situation actuelle",
                ),
                Some(copy(
                    language,
                    "Share the context you want someone to review.",
                    "Présentez le contexte que vous souhaitez faire examiner.",
                )),
                true,
                TextPresentation::Multiline,
            )?,
            text_field(
                "field-diagnostic-goal",
                "diagnostic_goal",
                copy(
                    language,
                    "What would you like to clarify?",
                    "Que souhaitez-vous clarifier ?",
                ),
                Some(copy(
                    language,
                    "There is no score or automated assessment.",
                    "Aucun score ni évaluation automatisée n’est produit.",
                )),
                true,
                TextPresentation::Multiline,
            )?,
        ],
    };
    let (purpose, submit_label) = match service_recipe {
        None | Some(ServiceRecipe::Contact) => (
            copy(language, "Service inquiry", "Demande de service"),
            copy(language, "Send request", "Envoyer la demande"),
        ),
        Some(ServiceRecipe::QuoteRequest) => (
            copy(language, "Estimate request", "Demande de devis"),
            copy(language, "Request an estimate", "Demander un devis"),
        ),
        Some(ServiceRecipe::DiagnosticApplication) => (
            copy(language, "Diagnostic request", "Demande de diagnostic"),
            copy(
                language,
                "Send diagnostic request",
                "Envoyer la demande de diagnostic",
            ),
        ),
    };

    Ok(FormV2 {
        id: FormId::parse("form-service-inquiry")?,
        fields,
        purpose,
        request_consent: ConsentCopy {
            copy_version: StableDocumentId::parse(consent_version.to_owned())?,
            copy: consent_copy.to_owned(),
        },
        marketing_opt_in: None,
        result: FormResult::SuccessPage {
            page_id: success_page,
        },
        presentation: Some(FormPresentation {
            submit_label: Some(submit_label),
            information_url: None,
        }),
    })
}

pub(super) fn service_request_confirmation(
    language: PublicLanguage,
    service_recipe: Option<ServiceRecipe>,
) -> Result<PageV2, DocumentError> {
    let (title, heading, follow_up, boundary) = match service_recipe {
        None | Some(ServiceRecipe::Contact) => (
            copy(language, "Request received", "Demande reçue"),
            copy(
                language,
                "Your request has been received.",
                "Votre demande a bien été reçue.",
            ),
            copy(
                language,
                "The service provider can review your request and contact you about next steps.",
                "Le prestataire pourra étudier votre demande et vous recontacter au sujet de la suite.",
            ),
            copy(
                language,
                "This confirms receipt only; it is not a reservation or a confirmed service.",
                "Ce message confirme seulement la réception de votre demande ; il ne s’agit ni d’une réservation ni d’une prestation confirmée.",
            ),
        ),
        Some(ServiceRecipe::QuoteRequest) => (
            copy(language, "Estimate request received", "Demande de devis reçue"),
            copy(language, "Your estimate request has been received.", "Votre demande de devis a bien été reçue."),
            copy(
                language,
                "The service provider can review the scope and decide whether an estimate is appropriate.",
                "Le prestataire pourra étudier le périmètre et déterminer si un devis est adapté.",
            ),
            copy(
                language,
                "This confirms receipt only; no price or response date is promised.",
                "Ce message confirme seulement la réception ; aucun prix ni délai de réponse n’est promis.",
            ),
        ),
        Some(ServiceRecipe::DiagnosticApplication) => (
            copy(language, "Diagnostic request received", "Demande de diagnostic reçue"),
            copy(
                language,
                "Your request has been received for review.",
                "Votre demande a bien été reçue pour examen.",
            ),
            copy(
                language,
                "The service provider can review the context and decide whether a conversation is appropriate.",
                "Le prestataire pourra examiner le contexte et déterminer si un échange est adapté.",
            ),
            copy(
                language,
                "This does not book a session or provide a diagnosis.",
                "Cette confirmation ne réserve pas de séance et ne fournit pas de diagnostic.",
            ),
        ),
    };

    Ok(PageV2 {
        id: PageId::parse("page-request-received")?,
        slug: "request-received".to_owned(),
        title,
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
                    text: heading,
                },
                BlockV2::Text {
                    id: BlockId::parse("block-request-received-follow-up")?,
                    text: follow_up,
                },
                BlockV2::Text {
                    id: BlockId::parse("block-request-received-boundary")?,
                    text: boundary,
                },
            ],
        }],
    })
}

fn name_field(language: PublicLanguage) -> Result<FormField, DocumentError> {
    Ok(FormField::Text {
        id: FieldId::parse("field-name")?,
        name: "name".to_owned(),
        label: copy(language, "Your name", "Votre nom"),
        help: None,
        required: false,
        validation: None,
        presentation: TextPresentation::SingleLine,
    })
}

fn email_field(language: PublicLanguage) -> Result<FormField, DocumentError> {
    Ok(FormField::Email {
        id: FieldId::parse("field-email")?,
        name: "email".to_owned(),
        label: copy(language, "Email address", "Adresse e-mail"),
        help: None,
        required: true,
        validation: None,
    })
}

fn text_field(
    id: &str,
    name: &str,
    label: String,
    help: Option<String>,
    required: bool,
    presentation: TextPresentation,
) -> Result<FormField, DocumentError> {
    Ok(FormField::Text {
        id: FieldId::parse(id)?,
        name: name.to_owned(),
        label,
        help,
        required,
        validation: None,
        presentation,
    })
}

fn service_copy(
    language: PublicLanguage,
    service_recipe: Option<ServiceRecipe>,
    contact: (&str, &str),
    quote_request: (&str, &str),
    diagnostic_application: (&str, &str),
) -> String {
    let (english, french) = match service_recipe {
        None | Some(ServiceRecipe::Contact) => contact,
        Some(ServiceRecipe::QuoteRequest) => quote_request,
        Some(ServiceRecipe::DiagnosticApplication) => diagnostic_application,
    };
    copy(language, english, french)
}

fn copy(language: PublicLanguage, english: &str, french: &str) -> String {
    match language {
        PublicLanguage::English => english,
        PublicLanguage::French => french,
    }
    .to_owned()
}
