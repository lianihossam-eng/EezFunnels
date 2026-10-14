const payloadElement = document.getElementById("dioxus-hydration-data");

if (!(payloadElement instanceof HTMLScriptElement)) {
  throw new Error("Dashboard hydration data is missing");
}

const payload = JSON.parse(payloadElement.textContent ?? "null");
const base64Pattern = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

if (
  typeof payload?.data !== "string" ||
  payload.data.length === 0 ||
  !base64Pattern.test(payload.data)
) {
  throw new Error("Dashboard hydration data is invalid");
}

window.initial_dioxus_hydration_data = payload.data;

if (Array.isArray(payload.debug_types)) {
  window.initial_dioxus_hydration_debug_types = payload.debug_types;
}

if (Array.isArray(payload.debug_locations)) {
  window.initial_dioxus_hydration_debug_locations = payload.debug_locations;
}

const wasm = await import("./eezfunnel.js");
await wasm.default();
