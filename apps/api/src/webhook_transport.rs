use eez_commands::public_delivery::{WebhookTransport, WebhookTransportResult};
use eez_domain::webhook_delivery::SignedWebhookDelivery;
use std::future::Future;
use std::pin::Pin;
use worker::{Fetch, Headers, Method, Request, RequestInit, RequestRedirect};

pub struct CloudflareWebhookTransport;

impl WebhookTransport for CloudflareWebhookTransport {
    fn post<'a>(
        &'a self,
        delivery: SignedWebhookDelivery,
    ) -> Pin<Box<dyn Future<Output = WebhookTransportResult> + 'a>> {
        Box::pin(async move {
            let headers = Headers::new();
            if headers.set("content-type", "application/json").is_err()
                || headers
                    .set("x-eezfunnels-delivery-id", delivery.delivery_id())
                    .is_err()
                || headers
                    .set("x-eezfunnels-timestamp", &delivery.timestamp().to_string())
                    .is_err()
                || headers
                    .set(
                        "x-eezfunnels-signature",
                        &format!("v1={}", delivery.signature()),
                    )
                    .is_err()
            {
                return WebhookTransportResult::InvalidRequest;
            }

            let Ok(body) = std::str::from_utf8(delivery.body()) else {
                return WebhookTransportResult::InvalidRequest;
            };

            let mut init = RequestInit::new();
            init.with_method(Method::Post)
                .with_headers(headers)
                .with_redirect(RequestRedirect::Manual)
                .with_body(Some(worker::wasm_bindgen::JsValue::from_str(body)));
            let Ok(request) = Request::new_with_init(delivery.destination(), &init) else {
                return WebhookTransportResult::InvalidRequest;
            };
            let signal =
                worker::AbortSignal::from(worker::web_sys::AbortSignal::timeout_with_u32(10_000));
            match Fetch::Request(request).send_with_signal(&signal).await {
                Ok(response) => WebhookTransportResult::HttpStatus(response.status_code()),
                Err(_) => WebhookTransportResult::NetworkFailure,
            }
        })
    }
}
