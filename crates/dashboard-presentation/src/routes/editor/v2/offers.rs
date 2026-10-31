use super::orchestration::dispatch_v2_edit;
use super::state::V2EditorState;
use super::*;
use eez_domain::document_v2::MAX_OFFER_UNIT_BYTES;

#[cfg(feature = "browser")]
pub(super) fn offer_choices(document: &DocumentV2) -> Vec<(BlockId, String)> {
    document
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .flat_map(|section| section.blocks())
        .filter_map(|block| match block {
            BlockV2::OfferCard { id, title, .. } => Some((
                id.clone(),
                if title.trim().is_empty() {
                    "Untitled offer".to_owned()
                } else {
                    title.clone()
                },
            )),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "browser")]
fn offer_input_key(block_id: &BlockId, field: &str) -> String {
    format!("offer:{}:{field}", block_id.as_str())
}

#[cfg(feature = "browser")]
fn offer_input_value(state: Signal<V2EditorState>, key: &str, fallback: &str) -> String {
    state.with(|state| {
        state
            .form_input_drafts
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    })
}

#[cfg(feature = "browser")]
fn offer_input_error(state: Signal<V2EditorState>, key: &str) -> Option<String> {
    state.with(|state| state.form_input_errors.get(key).cloned())
}

#[cfg(feature = "browser")]
fn set_offer_input_draft(
    mut state: Signal<V2EditorState>,
    key: String,
    value: String,
    error: Option<&'static str>,
) {
    state.with_mut(|state| {
        state.form_input_drafts.insert(key.clone(), value);
        if let Some(error) = error {
            state.form_input_errors.insert(key, error.to_owned());
        } else {
            state.form_input_errors.remove(&key);
        }
    });
}

#[cfg(feature = "browser")]
fn clear_offer_input_draft(mut state: Signal<V2EditorState>, key: String) {
    state.with_mut(|state| {
        state.form_input_drafts.remove(&key);
        state.form_input_errors.remove(&key);
    });
}

#[cfg(feature = "browser")]
fn offer_price_mode(price: Option<&OfferPriceV2>) -> &'static str {
    match price {
        Some(OfferPriceV2::OneTime { .. }) => "one_time",
        Some(OfferPriceV2::Recurring { .. }) => "recurring",
        None => "unset",
    }
}

#[cfg(feature = "browser")]
fn offer_price_currency(price: &OfferPriceV2) -> Option<CurrencyCode> {
    match price {
        OfferPriceV2::OneTime { currency, .. } | OfferPriceV2::Recurring { currency, .. } => {
            currency.clone()
        }
    }
}

#[cfg(feature = "browser")]
fn offer_price_unit(price: &OfferPriceV2) -> String {
    match price {
        OfferPriceV2::OneTime { unit, .. } | OfferPriceV2::Recurring { unit, .. } => unit.clone(),
    }
}

#[cfg(feature = "browser")]
fn offer_price_with_currency(price: &OfferPriceV2, currency: Option<CurrencyCode>) -> OfferPriceV2 {
    match price.clone() {
        OfferPriceV2::OneTime { amount, unit, .. } => OfferPriceV2::OneTime {
            amount,
            currency,
            unit,
        },
        OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            unit,
            ..
        } => OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            currency,
            unit,
        },
    }
}

#[cfg(feature = "browser")]
fn offer_price_with_unit(price: &OfferPriceV2, unit: String) -> OfferPriceV2 {
    match price.clone() {
        OfferPriceV2::OneTime {
            amount, currency, ..
        } => OfferPriceV2::OneTime {
            amount,
            currency,
            unit,
        },
        OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            currency,
            ..
        } => OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            currency,
            unit,
        },
    }
}

#[cfg(feature = "browser")]
fn offer_price_with_amount(
    price: &OfferPriceV2,
    field: &str,
    amount: Option<ExactPriceAmount>,
) -> OfferPriceV2 {
    match price.clone() {
        OfferPriceV2::OneTime { currency, unit, .. } => OfferPriceV2::OneTime {
            amount,
            currency,
            unit,
        },
        OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            currency,
            unit,
        } => {
            let (initial_amount, recurring_amount, announced_total) = match field {
                "initial_amount" => (amount, recurring_amount, announced_total),
                "recurring_amount" => (initial_amount, amount, announced_total),
                "announced_total" => (initial_amount, recurring_amount, amount),
                _ => (initial_amount, recurring_amount, announced_total),
            };
            OfferPriceV2::Recurring {
                initial_amount,
                recurring_amount,
                period,
                announced_total,
                currency,
                unit,
            }
        }
    }
}

#[cfg(feature = "browser")]
fn offer_price_with_period(price: &OfferPriceV2, period: PricePeriod) -> OfferPriceV2 {
    match price.clone() {
        OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            announced_total,
            currency,
            unit,
            ..
        } => OfferPriceV2::Recurring {
            initial_amount,
            recurring_amount,
            period,
            announced_total,
            currency,
            unit,
        },
        one_time => one_time,
    }
}

#[cfg(feature = "browser")]
fn parse_price_amount_input(value: &str) -> Result<Option<ExactPriceAmount>, &'static str> {
    if value.is_empty() {
        Ok(None)
    } else {
        ExactPriceAmount::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a non-negative decimal such as 1250.50, with no symbols or grouping.")
    }
}

#[cfg(feature = "browser")]
fn parse_currency_input(value: &str) -> Result<Option<CurrencyCode>, &'static str> {
    if value.is_empty() {
        Ok(None)
    } else {
        CurrencyCode::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use three uppercase letters such as EUR.")
    }
}

#[cfg(feature = "browser")]
fn set_offer_card(
    state: Signal<V2EditorState>,
    block: BlockId,
    title: String,
    scope: String,
    price: Option<OfferPriceV2>,
    conditions: String,
) {
    dispatch_v2_edit(
        state,
        Edit::SetBlock {
            block,
            value: NewBlock::OfferCard {
                title,
                scope,
                price,
                conditions,
            },
        },
    );
}

#[cfg(feature = "browser")]
fn offer_period_id(period: PricePeriod) -> &'static str {
    match period {
        PricePeriod::Weekly => "weekly",
        PricePeriod::Monthly => "monthly",
        PricePeriod::Quarterly => "quarterly",
        PricePeriod::Yearly => "yearly",
    }
}

#[cfg(feature = "browser")]
fn offer_period_from_id(value: &str) -> Option<PricePeriod> {
    match value {
        "weekly" => Some(PricePeriod::Weekly),
        "monthly" => Some(PricePeriod::Monthly),
        "quarterly" => Some(PricePeriod::Quarterly),
        "yearly" => Some(PricePeriod::Yearly),
        _ => None,
    }
}

#[cfg(feature = "browser")]
#[component]
pub(super) fn V2OfferCardEditor(
    state: Signal<V2EditorState>,
    block: BlockV2,
    locked: bool,
) -> Element {
    let BlockV2::OfferCard {
        id,
        title,
        scope,
        price,
        conditions,
    } = block
    else {
        return rsx! {};
    };
    let selected_mode = offer_price_mode(price.as_ref());
    let current_price = price.clone();
    let title_id = id.clone();
    let title_scope = scope.clone();
    let title_price = price.clone();
    let title_conditions = conditions.clone();
    let scope_id = id.clone();
    let scope_title = title.clone();
    let scope_price = price.clone();
    let scope_conditions = conditions.clone();
    let conditions_id = id.clone();
    let conditions_title = title.clone();
    let conditions_scope = scope.clone();
    let conditions_price = price.clone();
    let mode_id = id.clone();
    let mode_title = title.clone();
    let mode_scope = scope.clone();
    let mode_conditions = conditions.clone();
    let period_id = id.clone();
    let period_title = title.clone();
    let period_scope = scope.clone();
    let period_price = price.clone();
    let period_conditions = conditions.clone();
    rsx! {
        fieldset { class: "editor-setting-group",
            legend { "Offer details" }
            p { class: "editor-keyboard-help", "Complete the title, included scope, exact price, and conditions before publishing. Amounts are exact decimals, never calculated from other values." }
            label { class: "editor-setting-control",
                "Offer name"
                input {
                    r#type: "text",
                    value: "{title}",
                    disabled: locked,
                    "aria-label": "Offer name for {id.as_str()}",
                    onchange: move |event| set_offer_card(state, title_id.clone(), event.value(), title_scope.clone(), title_price.clone(), title_conditions.clone()),
                }
            }
            label { class: "editor-setting-control",
                "Included scope"
                textarea {
                    rows: "3",
                    value: "{scope}",
                    disabled: locked,
                    "aria-label": "Included scope for offer {id.as_str()}",
                    onchange: move |event| set_offer_card(state, scope_id.clone(), scope_title.clone(), event.value(), scope_price.clone(), scope_conditions.clone()),
                }
            }
            label { class: "editor-setting-control",
                "Conditions"
                textarea {
                    rows: "3",
                    value: "{conditions}",
                    disabled: locked,
                    "aria-label": "Conditions for offer {id.as_str()}",
                    onchange: move |event| set_offer_card(state, conditions_id.clone(), conditions_title.clone(), conditions_scope.clone(), conditions_price.clone(), event.value()),
                }
            }
            label { class: "editor-setting-control",
                "Price structure"
                select {
                    value: "{selected_mode}",
                    disabled: locked,
                    "aria-label": "Price structure for offer {id.as_str()}",
                    onchange: move |event| {
                        let selected = event.value();
                        let current_currency = current_price.as_ref().and_then(offer_price_currency);
                        let current_unit = current_price.as_ref().map(offer_price_unit).unwrap_or_default();
                        let next_price = match selected.as_str() {
                            "one_time" => match current_price.clone() {
                                Some(OfferPriceV2::OneTime { .. }) => current_price.clone(),
                                _ => Some(OfferPriceV2::OneTime {
                                    amount: None,
                                    currency: current_currency,
                                    unit: current_unit,
                                }),
                            },
                            "recurring" => match current_price.clone() {
                                Some(OfferPriceV2::Recurring { .. }) => current_price.clone(),
                                _ => Some(OfferPriceV2::Recurring {
                                    initial_amount: None,
                                    recurring_amount: None,
                                    period: PricePeriod::Monthly,
                                    announced_total: None,
                                    currency: current_currency,
                                    unit: current_unit,
                                }),
                            },
                            _ => None,
                        };
                        set_offer_card(state, mode_id.clone(), mode_title.clone(), mode_scope.clone(), next_price, mode_conditions.clone());
                    },
                    option { value: "unset", "Choose a price structure" }
                    option { value: "one_time", "One-time price" }
                    option { value: "recurring", "Recurring price" }
                }
            }
            if let Some(price) = price.as_ref() {
                V2OfferCurrencyField {
                    state,
                    block_id: id.clone(),
                    title: title.clone(),
                    scope: scope.clone(),
                    price: price.clone(),
                    conditions: conditions.clone(),
                    locked,
                }
                V2OfferUnitField {
                    state,
                    block_id: id.clone(),
                    title: title.clone(),
                    scope: scope.clone(),
                    price: price.clone(),
                    conditions: conditions.clone(),
                    locked,
                }
                match price {
                    OfferPriceV2::OneTime { amount, .. } => rsx! {
                        V2OfferAmountField {
                            state,
                            block_id: id.clone(),
                            title: title.clone(),
                            scope: scope.clone(),
                            price: price.clone(),
                            conditions: conditions.clone(),
                            field: "amount".to_owned(),
                            label: "One-time amount".to_owned(),
                            amount: amount.clone(),
                            locked,
                        }
                    },
                    OfferPriceV2::Recurring {
                        initial_amount,
                        recurring_amount,
                        period,
                        announced_total,
                        ..
                    } => rsx! {
                        V2OfferAmountField {
                            state,
                            block_id: id.clone(),
                            title: title.clone(),
                            scope: scope.clone(),
                            price: price.clone(),
                            conditions: conditions.clone(),
                            field: "initial_amount".to_owned(),
                            label: "Initial amount (optional)".to_owned(),
                            amount: initial_amount.clone(),
                            locked,
                        }
                        V2OfferAmountField {
                            state,
                            block_id: id.clone(),
                            title: title.clone(),
                            scope: scope.clone(),
                            price: price.clone(),
                            conditions: conditions.clone(),
                            field: "recurring_amount".to_owned(),
                            label: "Recurring amount".to_owned(),
                            amount: recurring_amount.clone(),
                            locked,
                        }
                        label { class: "editor-setting-control",
                            "Recurrence period"
                            select {
                                value: "{offer_period_id(*period)}",
                                disabled: locked,
                                onchange: move |event| {
                                    if let Some(period) = offer_period_from_id(&event.value()) {
                                        if let Some(current_price) = period_price.as_ref() {
                                            let next_price = offer_price_with_period(current_price, period);
                                            set_offer_card(state, period_id.clone(), period_title.clone(), period_scope.clone(), Some(next_price), period_conditions.clone());
                                        }
                                    }
                                },
                                option { value: "weekly", "Weekly" }
                                option { value: "monthly", "Monthly" }
                                option { value: "quarterly", "Quarterly" }
                                option { value: "yearly", "Yearly" }
                            }
                        }
                        V2OfferAmountField {
                            state,
                            block_id: id.clone(),
                            title: title.clone(),
                            scope: scope.clone(),
                            price: price.clone(),
                            conditions: conditions.clone(),
                            field: "announced_total".to_owned(),
                            label: "Announced total (optional; never calculated)".to_owned(),
                            amount: announced_total.clone(),
                            locked,
                        }
                    },
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2OfferCurrencyField(
    state: Signal<V2EditorState>,
    block_id: BlockId,
    title: String,
    scope: String,
    price: OfferPriceV2,
    conditions: String,
    locked: bool,
) -> Element {
    let key = offer_input_key(&block_id, "currency");
    let fallback = offer_price_currency(&price)
        .as_ref()
        .map(CurrencyCode::as_str)
        .unwrap_or_default()
        .to_owned();
    let value = offer_input_value(state, &key, &fallback);
    let error = offer_input_error(state, &key);
    let error_id = format!("offer-currency-error-{}", block_id.as_str());
    let error_text = error.as_deref().unwrap_or_default();
    rsx! {
        label { class: "editor-setting-control",
            "Currency code"
            input {
                r#type: "text",
                value: "{value}",
                disabled: locked,
                pattern: "[A-Z]{3}",
                title: "Three uppercase letters, for example EUR",
                "aria-label": "Currency code for offer {block_id.as_str()}",
                aria_describedby: "{error_id}",
                onchange: move |event| {
                    let input = event.value();
                    match parse_currency_input(&input) {
                        Ok(currency) => {
                            clear_offer_input_draft(state, key.clone());
                            let next_price = offer_price_with_currency(&price, currency);
                            set_offer_card(state, block_id.clone(), title.clone(), scope.clone(), Some(next_price), conditions.clone());
                        }
                        Err(message) => set_offer_input_draft(state, key.clone(), input, Some(message)),
                    }
                },
            }
            span { id: "{error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{error_text}" }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2OfferUnitField(
    state: Signal<V2EditorState>,
    block_id: BlockId,
    title: String,
    scope: String,
    price: OfferPriceV2,
    conditions: String,
    locked: bool,
) -> Element {
    let key = offer_input_key(&block_id, "unit");
    let fallback = offer_price_unit(&price);
    let value = offer_input_value(state, &key, &fallback);
    let error = offer_input_error(state, &key);
    let error_id = format!("offer-unit-error-{}", block_id.as_str());
    let byte_count = value.len();
    let error_text = error.as_deref().unwrap_or_default();
    rsx! {
        label { class: "editor-setting-control",
            "Price unit"
            input {
                r#type: "text",
                value: "{value}",
                disabled: locked,
                "aria-label": "Price unit for offer {block_id.as_str()}",
                aria_describedby: "{error_id}",
                onchange: move |event| {
                    let input = event.value();
                    if input.len() > MAX_OFFER_UNIT_BYTES {
                        set_offer_input_draft(state, key.clone(), input, Some("Price unit must fit within 80 UTF-8 bytes."));
                    } else {
                        clear_offer_input_draft(state, key.clone());
                        let next_price = offer_price_with_unit(&price, input);
                        set_offer_card(state, block_id.clone(), title.clone(), scope.clone(), Some(next_price), conditions.clone());
                    }
                },
            }
            span { class: "editor-keyboard-help", "{byte_count} / {MAX_OFFER_UNIT_BYTES} UTF-8 bytes; shown after “per”." }
            span { id: "{error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{error_text}" }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2OfferAmountField(
    state: Signal<V2EditorState>,
    block_id: BlockId,
    title: String,
    scope: String,
    price: OfferPriceV2,
    conditions: String,
    field: String,
    label: String,
    amount: Option<ExactPriceAmount>,
    locked: bool,
) -> Element {
    let key = offer_input_key(&block_id, &field);
    let fallback = amount
        .as_ref()
        .map(ExactPriceAmount::as_str)
        .unwrap_or_default();
    let value = offer_input_value(state, &key, fallback);
    let error = offer_input_error(state, &key);
    let error_id = format!("offer-amount-error-{}-{field}", block_id.as_str());
    let hint_id = format!("offer-amount-hint-{}-{field}", block_id.as_str());
    let price_for_edit = price.clone();
    let error_text = error.as_deref().unwrap_or_default();
    rsx! {
        label { class: "editor-setting-control",
            "{label}"
            input {
                r#type: "text",
                inputmode: "decimal",
                value: "{value}",
                disabled: locked,
                aria_describedby: "{hint_id} {error_id}",
                onchange: move |event| {
                    let input = event.value();
                    match parse_price_amount_input(&input) {
                        Ok(amount) => {
                            clear_offer_input_draft(state, key.clone());
                            let next_price = offer_price_with_amount(&price_for_edit, &field, amount);
                            set_offer_card(state, block_id.clone(), title.clone(), scope.clone(), Some(next_price), conditions.clone());
                        }
                        Err(message) => set_offer_input_draft(state, key.clone(), input, Some(message)),
                    }
                },
            }
            span { id: "{hint_id}", class: "editor-keyboard-help", "Exact decimal only (for example 1250.50); up to 12 whole digits and 3 decimal places. No currency symbols or separators." }
            span { id: "{error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{error_text}" }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
pub(super) fn V2OfferComparisonEditor(
    state: Signal<V2EditorState>,
    block: BlockV2,
    locked: bool,
) -> Element {
    let BlockV2::OfferComparison { id, offer_ids } = block else {
        return rsx! {};
    };
    let choices = state.with(|state| {
        state
            .document
            .as_ref()
            .map(offer_choices)
            .unwrap_or_default()
    });
    let selected_count = offer_ids.len();
    rsx! {
        fieldset { class: "editor-setting-group",
            legend { "Offers in comparison" }
            p { class: "editor-keyboard-help", "Choose 2–{MAX_OFFER_COMPARISON_ITEMS} offer cards. The comparison reads the same source values as each card." }
            for (offer_id, title) in choices {
                {
                    let checked = offer_ids.contains(&offer_id);
                    let disabled = locked
                        || checked && selected_count <= 2
                        || !checked && selected_count >= MAX_OFFER_COMPARISON_ITEMS;
                    let comparison_block = id.clone();
                    let offer_ids_for_edit = offer_ids.clone();
                    let offer_id_for_edit = offer_id.clone();
                    rsx! {
                        label { class: "editor-setting-control editor-offer-choice",
                            input {
                                r#type: "checkbox",
                                checked,
                                disabled,
                                "aria-label": "Include offer {title} in comparison",
                                onchange: move |event| {
                                    let mut next = offer_ids_for_edit.clone();
                                    if event.checked() {
                                        if next.len() < MAX_OFFER_COMPARISON_ITEMS && !next.contains(&offer_id_for_edit) {
                                            next.push(offer_id_for_edit.clone());
                                        }
                                    } else if next.len() > 2 {
                                        next.retain(|item| item != &offer_id_for_edit);
                                    }
                                    dispatch_v2_edit(state, Edit::SetBlock {
                                        block: comparison_block.clone(),
                                        value: NewBlock::OfferComparison { offer_ids: next },
                                    });
                                },
                            }
                            "{title}"
                        }
                    }
                }
            }
            p { class: "editor-keyboard-help", "At least two offers must remain selected; delete this comparison if it is no longer needed." }
        }
    }
}
