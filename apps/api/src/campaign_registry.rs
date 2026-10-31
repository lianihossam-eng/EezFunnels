use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use eez_commands::campaign_registry::{
    self, CampaignRegistryCommandError, CampaignRegistrySnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;
use worker::{Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::validate_unique_json;

const MAX_CAMPAIGN_REGISTRY_REQUEST_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CampaignItemRoute<'a> {
    pub(super) funnel_id: &'a str,
    pub(super) campaign_id: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddCampaignBody {
    campaign_id: String,
}

#[derive(Serialize)]
struct CampaignLinkResponse {
    campaign_id: String,
    url: Option<String>,
    relative_path: Option<String>,
}

pub(super) fn item_from_path(path: &str) -> Option<CampaignItemRoute<'_>> {
    let mut segments = path.strip_prefix("/api/funnels/")?.split('/');
    let funnel_id = segments.next()?;
    if segments.next()? != "campaigns" {
        return None;
    }
    let campaign_id = segments.next()?;
    if funnel_id.is_empty() || campaign_id.is_empty() || segments.next().is_some() {
        return None;
    }
    Some(CampaignItemRoute {
        funnel_id,
        campaign_id,
    })
}

pub(super) async fn collection_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::valid_funnel_route_id(funnel_id) {
        return private_error_response("not_found/funnel", correlation_id);
    }
    match *request.method() {
        Method::GET => read_response(env, funnel_id, correlation_id).await,
        Method::POST => add_response(env, request, funnel_id, correlation_id).await,
        _ => crate::private_lifecycle_response(crate::method_not_allowed_for(
            correlation_id,
            "GET, POST",
        )),
    }
}

pub(super) async fn item_response(
    env: &Env,
    request: HttpRequest,
    route: CampaignItemRoute<'_>,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::valid_funnel_route_id(route.funnel_id) {
        return private_error_response("not_found/funnel", correlation_id);
    }
    if request.method() != Method::DELETE {
        return crate::private_lifecycle_response(crate::method_not_allowed_for(
            correlation_id,
            "DELETE",
        ));
    }
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return private_error_response("request/invalid_origin", correlation_id);
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match campaign_registry::remove_campaign_id(
        &bindings.repository(),
        &bindings.campaign_registry_repository(),
        route.funnel_id,
        route.campaign_id,
    )
    .await
    {
        Ok(snapshot) => success_response(env, snapshot, correlation_id),
        Err(error) => command_error_response(error, correlation_id),
    }
}

async fn read_response(
    env: &Env,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match campaign_registry::read_campaign_registry(
        &bindings.repository(),
        &bindings.campaign_registry_repository(),
        funnel_id,
    )
    .await
    {
        Ok(snapshot) => success_response(env, snapshot, correlation_id),
        Err(error) => command_error_response(error, correlation_id),
    }
}

async fn add_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return private_error_response("request/invalid_origin", correlation_id);
    }
    if !crate::is_json_content_type(request.headers()) {
        return private_error_response("validation/invalid_json", correlation_id);
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_CAMPAIGN_REGISTRY_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let input = match parse_add_body(body) {
        Ok(input) => input,
        Err(()) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match campaign_registry::add_campaign_id(
        &bindings.repository(),
        &bindings.campaign_registry_repository(),
        funnel_id,
        &input.campaign_id,
    )
    .await
    {
        Ok(snapshot) => success_response(env, snapshot, correlation_id),
        Err(error) => command_error_response(error, correlation_id),
    }
}

fn parse_add_body(body: &str) -> Result<AddCampaignBody, ()> {
    validate_unique_json(body).map_err(|_| ())?;
    serde_json::from_str(body).map_err(|_| ())
}

fn success_response(
    env: &Env,
    snapshot: CampaignRegistrySnapshot,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let link_base = campaign_link_base(env, &snapshot.funnel_slug, snapshot.published);
    let relative_base = valid_relative_campaign_base(&snapshot.funnel_slug, snapshot.published);
    let campaign_links = snapshot
        .campaign_ids
        .iter()
        .map(|campaign_id| CampaignLinkResponse {
            campaign_id: campaign_id.clone(),
            url: link_base
                .as_deref()
                .and_then(|base| campaign_link(base, campaign_id)),
            relative_path: relative_base
                .as_deref()
                .map(|base| format!("{base}?campaign_id={campaign_id}")),
        })
        .collect::<Vec<_>>();
    let result = json!({
        "funnel_id": snapshot.funnel_id,
        "campaign_ids": snapshot.campaign_ids,
        "max_campaign_ids": snapshot.max_campaign_ids,
        "published": snapshot.published,
        "campaign_links": campaign_links,
    });
    crate::private_lifecycle_response(crate::json_success_response(
        result,
        StatusCode::OK,
        correlation_id,
        None,
    ))
}

fn campaign_link_base(env: &Env, funnel_slug: &str, published: bool) -> Option<String> {
    if !published {
        return None;
    }
    let (_, custom_hosts) = crate::request_host_configuration(env);
    let custom_origin = custom_hosts
        .mapping_for_funnel(funnel_slug)
        .map(|mapping| crate::CustomHostMappings::origin(&mapping));
    let configured_origin = env
        .var("PUBLIC_BASE_URL")
        .ok()
        .map(|value| value.to_string());
    let origin = custom_origin.or(configured_origin);
    crate::canonical::public_page_url(origin.as_deref(), funnel_slug, None)
}

fn valid_relative_campaign_base(funnel_slug: &str, published: bool) -> Option<String> {
    if published && crate::valid_public_slug(funnel_slug) {
        Some(format!("/f/{funnel_slug}"))
    } else {
        None
    }
}

fn campaign_link(base: &str, campaign_id: &str) -> Option<String> {
    let mut url = Url::parse(base).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    url.query_pairs_mut()
        .append_pair("campaign_id", campaign_id);
    Some(url.to_string())
}

fn command_error_response(
    error: CampaignRegistryCommandError,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    private_error_response(error.code(), correlation_id)
}

fn private_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    crate::log_api_error(&error, correlation_id, error.code());
    crate::private_lifecycle_response(crate::error_response(&error, correlation_id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{campaign_link, item_from_path, parse_add_body, valid_relative_campaign_base};
    use crate::json::tests::deterministic_json_fuzz_inputs;
    use std::panic::catch_unwind;

    #[test]
    fn add_parser_accepts_one_campaign_id_and_rejects_ambiguous_or_extra_fields() {
        assert_eq!(
            parse_add_body(r#"{"campaign_id":"spring-launch"}"#).map(|input| input.campaign_id),
            Ok("spring-launch".to_owned())
        );
        assert!(parse_add_body(r#"{"campaign_id":"first","campaign_id":"second"}"#).is_err());
        assert!(parse_add_body(r#"{"campaign_id":"spring-launch","label":"Spring"}"#).is_err());
    }

    #[test]
    fn campaign_item_routes_are_exact_and_do_not_accept_extra_segments() {
        assert_eq!(
            item_from_path("/api/funnels/funnel-01/campaigns/spring-launch"),
            Some(super::CampaignItemRoute {
                funnel_id: "funnel-01",
                campaign_id: "spring-launch",
            })
        );
        for path in [
            "/api/funnels/funnel-01/campaigns",
            "/api/funnels//campaigns/spring-launch",
            "/api/funnels/funnel-01/campaigns/spring-launch/extra",
            "/api/funnels/funnel-01/other/spring-launch",
        ] {
            assert_eq!(item_from_path(path), None, "accepted {path:?}");
        }
    }

    #[test]
    fn generated_links_use_only_the_validated_campaign_id_and_published_entry_path() {
        assert_eq!(
            campaign_link("https://funnels.example/f/lead-gen", "spring-launch"),
            Some("https://funnels.example/f/lead-gen?campaign_id=spring-launch".to_owned())
        );
        assert_eq!(
            campaign_link("http://funnels.example/f/lead-gen", "spring-launch"),
            None
        );
        assert_eq!(
            valid_relative_campaign_base("lead-gen", true),
            Some("/f/lead-gen".to_owned())
        );
        assert_eq!(valid_relative_campaign_base("lead-gen", false), None);
    }

    #[test]
    fn campaign_request_json_fuzz_inputs_never_panic() {
        for (index, input) in deterministic_json_fuzz_inputs().iter().enumerate() {
            let result = catch_unwind(|| parse_add_body(input));
            assert!(
                result.is_ok(),
                "parser panicked for case {index}: {input:?}"
            );
        }
    }
}
