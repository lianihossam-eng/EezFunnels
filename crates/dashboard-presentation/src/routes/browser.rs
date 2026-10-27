#[cfg(feature = "browser")]
use dioxus::prelude::*;

#[cfg(feature = "browser")]
const BROWSER_MODAL_DIALOG: &str = r#"
const {id, open} = await dioxus.recv();
const dialog = document.getElementById(id);
if (dialog instanceof HTMLDialogElement) {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
}
dioxus.send(true);
"#;

#[cfg(feature = "browser")]
pub(super) async fn set_modal_dialog(id: &'static str, open: bool) {
    let mut request = document::eval(BROWSER_MODAL_DIALOG);
    if request
        .send(serde_json::json!({ "id": id, "open": open }))
        .is_ok()
    {
        let _ = request.recv::<bool>().await;
    }
}
