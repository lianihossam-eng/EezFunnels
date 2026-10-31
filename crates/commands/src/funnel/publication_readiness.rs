use eez_domain::document_v2::{
    BlockV2, DocumentV2, GeneratedPlaceholderPageField, GeneratedPlaceholderTarget, OfferPriceV2,
};
use eez_storage::repository::RevisionRecord;
use serde::Serialize;

use super::{decode_snapshot, FunnelCommandError, SnapshotData};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct PublicationReadiness {
    revision_id: String,
    findings: Vec<ReadinessFinding>,
}

impl PublicationReadiness {
    pub(super) fn can_publish(&self) -> bool {
        self.findings
            .iter()
            .all(|finding| finding.severity != ReadinessSeverity::Blocking)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ReadinessFinding {
    code: &'static str,
    severity: ReadinessSeverity,
    action: ReadinessAction,
    target: ReadinessTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReadinessSeverity {
    Blocking,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReadinessAction {
    ReplaceOrRemoveGeneratedText,
    ReplaceActionDestination,
    AddSearchDescription,
    CompleteOfferDetails,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReadinessTarget {
    V1SectionField {
        section_id: String,
        field: String,
    },
    V2Block {
        page_id: String,
        section_id: String,
        block_id: String,
    },
    V2PageField {
        page_id: String,
        field: String,
    },
}

pub(super) fn for_revision(
    record: &RevisionRecord,
    funnel_id: &str,
) -> Result<PublicationReadiness, FunnelCommandError> {
    match record.schema_version {
        1 => {
            let validated = decode_snapshot(record, funnel_id)?;
            Ok(for_v1(&record.id, &validated.snapshot))
        }
        2 => {
            let document = DocumentV2::decode(record.data_json.as_bytes(), funnel_id)
                .map_err(|_| FunnelCommandError::Internal)?;
            Ok(for_v2(&record.id, &document))
        }
        _ => Err(FunnelCommandError::Internal),
    }
}

pub(super) fn for_v1(revision_id: &str, snapshot: &SnapshotData) -> PublicationReadiness {
    let findings = snapshot
        .generated_placeholders
        .iter()
        .map(|marker| ReadinessFinding {
            code: "publication/generated_placeholder_unresolved",
            severity: ReadinessSeverity::Blocking,
            action: ReadinessAction::ReplaceOrRemoveGeneratedText,
            target: ReadinessTarget::V1SectionField {
                section_id: marker.section_id.clone(),
                field: marker.field.clone(),
            },
        })
        .collect();
    PublicationReadiness {
        revision_id: revision_id.to_owned(),
        findings,
    }
}

pub(super) fn for_v2(revision_id: &str, document: &DocumentV2) -> PublicationReadiness {
    let mut findings = document
        .generated_placeholders()
        .iter()
        .map(|marker| ReadinessFinding {
            code: "publication/generated_placeholder_unresolved",
            severity: ReadinessSeverity::Blocking,
            action: ReadinessAction::ReplaceOrRemoveGeneratedText,
            target: match &marker.target {
                GeneratedPlaceholderTarget::Block {
                    page_id,
                    section_id,
                    block_id,
                } => ReadinessTarget::V2Block {
                    page_id: page_id.as_str().to_owned(),
                    section_id: section_id.as_str().to_owned(),
                    block_id: block_id.as_str().to_owned(),
                },
                GeneratedPlaceholderTarget::PageField { page_id, field } => {
                    ReadinessTarget::V2PageField {
                        page_id: page_id.as_str().to_owned(),
                        field: match field {
                            GeneratedPlaceholderPageField::Title => "title",
                            GeneratedPlaceholderPageField::SeoTitle => "seo_title",
                            GeneratedPlaceholderPageField::SeoDescription => "seo_description",
                        }
                        .to_owned(),
                    }
                }
            },
        })
        .collect::<Vec<_>>();
    for page in document.pages() {
        for section in page.sections() {
            for block in section.blocks() {
                if matches!(block, BlockV2::Button { action, .. } if action.is_reserved_sample()) {
                    findings.push(ReadinessFinding {
                        code: "publication/action_destination_unconfigured",
                        severity: ReadinessSeverity::Blocking,
                        action: ReadinessAction::ReplaceActionDestination,
                        target: ReadinessTarget::V2Block {
                            page_id: page.id().as_str().to_owned(),
                            section_id: section.id().as_str().to_owned(),
                            block_id: block.id().as_str().to_owned(),
                        },
                    });
                }
                if matches!(block, BlockV2::OfferCard { title, scope, price, conditions, .. }
                    if !offer_is_complete(title, scope, price.as_ref(), conditions))
                {
                    findings.push(ReadinessFinding {
                        code: "publication/offer_incomplete",
                        severity: ReadinessSeverity::Blocking,
                        action: ReadinessAction::CompleteOfferDetails,
                        target: ReadinessTarget::V2Block {
                            page_id: page.id().as_str().to_owned(),
                            section_id: section.id().as_str().to_owned(),
                            block_id: block.id().as_str().to_owned(),
                        },
                    });
                }
            }
        }
    }
    findings.extend(
        document
            .pages()
            .iter()
            .filter(|page| page.seo_description().is_none())
            .map(|page| ReadinessFinding {
                code: "editorial/search_description_missing",
                severity: ReadinessSeverity::Warning,
                action: ReadinessAction::AddSearchDescription,
                target: ReadinessTarget::V2PageField {
                    page_id: page.id().as_str().to_owned(),
                    field: "seo_description".to_owned(),
                },
            }),
    );
    PublicationReadiness {
        revision_id: revision_id.to_owned(),
        findings,
    }
}

fn offer_is_complete(
    title: &str,
    scope: &str,
    price: Option<&OfferPriceV2>,
    conditions: &str,
) -> bool {
    if title.trim().is_empty() || scope.trim().is_empty() || conditions.trim().is_empty() {
        return false;
    }
    match price {
        Some(OfferPriceV2::OneTime {
            amount: Some(_),
            currency: Some(_),
            unit,
            ..
        }) => !unit.trim().is_empty(),
        Some(OfferPriceV2::Recurring {
            recurring_amount: Some(_),
            currency: Some(_),
            unit,
            ..
        }) => !unit.trim().is_empty(),
        Some(OfferPriceV2::OneTime { .. }) | Some(OfferPriceV2::Recurring { .. }) | None => false,
    }
}
