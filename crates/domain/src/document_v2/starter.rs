use super::model::{
    ActionV2, BlockId, BlockV2, DocumentError, PublicLanguage, SectionId, SectionKind, SectionV2,
};

pub(super) fn starter_section(
    section_type: SectionKind,
    language: PublicLanguage,
) -> Result<SectionV2, DocumentError> {
    let (section_id, blocks) = match section_type {
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
                    text: copy(
                        language,
                        "Trusted by customers",
                        "La confiance de vos clients",
                    ),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-testimonials-copy")?,
                    text: copy(
                        language,
                        "Add a genuine customer story to build confidence.",
                        "Ajoutez un témoignage client authentique pour rassurer.",
                    ),
                },
            ],
        ),
        SectionKind::Cta => (
            "section-cta",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-cta-title")?,
                    level: 2,
                    text: copy(language, "Ready to get started?", "Prêt à commencer ?"),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-cta-copy")?,
                    text: copy(
                        language,
                        "Take the next step today.",
                        "Passez à l’étape suivante dès aujourd’hui.",
                    ),
                },
                BlockV2::Button {
                    id: BlockId::parse("block-cta-button")?,
                    label: copy(language, "Get started", "Commencer"),
                    action: ActionV2::ExternalHttps {
                        url: "https://example.com".to_owned(),
                    },
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

fn copy(language: PublicLanguage, english: &str, french: &str) -> String {
    match language {
        PublicLanguage::English => english,
        PublicLanguage::French => french,
    }
    .to_owned()
}
