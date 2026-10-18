(() => {
  "use strict";

  const script = document.currentScript;
  const page = document.querySelector("main#funnel");
  if (!script || !page) return;

  const funnelId = script.dataset.funnelId;
  const revisionId = script.dataset.revisionId;
  const endpoint = script.dataset.endpoint;
  const pageId = page.dataset.eezfunnelPageId || script.dataset.legacyPageId;
  if (!funnelId || !revisionId || !endpoint || !pageId) return;

  const choiceKey = "eezfunnel:interaction-consent:v1";
  const formElements = Array.from(
    document.querySelectorAll("form[data-eezfunnel-form-id]"),
  );
  const viewedForms = new Set();
  const startedForms = new Set();
  let pageViewed = false;
  let choice = null;

  try {
    const savedChoice = window.localStorage.getItem(choiceKey);
    if (savedChoice === "granted" || savedChoice === "refused") {
      choice = savedChoice;
    }
  } catch (_) {
    // Storage can be unavailable in private or restricted browsing contexts.
  }

  const style = document.createElement("style");
  style.textContent =
    "#eezfunnel-privacy{position:fixed;z-index:2147483000;inset:auto 1rem 1rem auto;max-width:min(32rem,calc(100vw - 2rem));font:16px/1.5 system-ui,sans-serif;color:#172033}" +
    "#eezfunnel-privacy button{font:inherit;cursor:pointer;border:1px solid #52607a;border-radius:.35rem;padding:.5rem .8rem;background:#fff;color:#172033}" +
    "#eezfunnel-privacy .eezfunnel-privacy-trigger{float:right;background:#172033;color:#fff}" +
    "#eezfunnel-privacy .eezfunnel-privacy-panel{clear:both;margin-top:.5rem;padding:1rem;border:1px solid #8993a6;border-radius:.5rem;background:#fff;box-shadow:0 4px 24px #17203333}" +
    "#eezfunnel-privacy .eezfunnel-privacy-actions{display:flex;flex-wrap:wrap;gap:.5rem;margin-top:.75rem}" +
    "#eezfunnel-privacy [hidden]{display:none!important}";
  document.head.appendChild(style);

  const controls = document.createElement("div");
  controls.id = "eezfunnel-privacy";
  controls.setAttribute("data-eezfunnel-privacy", "");
  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "eezfunnel-privacy-trigger";
  trigger.textContent = "Privacy choices";
  const panel = document.createElement("section");
  panel.className = "eezfunnel-privacy-panel";
  panel.setAttribute("aria-label", "Interaction measurement choices");
  panel.setAttribute("role", "region");
  const description = document.createElement("p");
  description.textContent =
    "Allow interaction measurement? We count published page and form interactions only. We do not store visitor or session IDs or form answers. Your choice is saved in this browser and can be changed at any time.";
  const actions = document.createElement("div");
  actions.className = "eezfunnel-privacy-actions";
  const allow = document.createElement("button");
  allow.type = "button";
  allow.textContent = "Allow measurement";
  const refuse = document.createElement("button");
  refuse.type = "button";
  refuse.textContent = "No thanks";
  actions.append(allow, refuse);
  panel.append(description, actions);
  controls.append(trigger, panel);
  document.body.appendChild(controls);
  panel.hidden = choice !== null;

  function campaignId() {
    try {
      const value = new URLSearchParams(window.location.search).get("campaign_id");
      return value && /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(value)
        ? value
        : null;
    } catch (_) {
      return null;
    }
  }

  function eventId() {
    if (window.crypto && typeof window.crypto.randomUUID === "function") {
      return window.crypto.randomUUID();
    }
    if (!window.crypto || typeof window.crypto.getRandomValues !== "function") {
      return null;
    }
    const bytes = window.crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
    return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex.slice(6, 8).join("")}-${hex.slice(8, 10).join("")}-${hex.slice(10).join("")}`;
  }

  function send(kind, formId) {
    if (choice !== "granted") return;
    const id = eventId();
    if (!id) return;
    const payload = {
      schema_version: 1,
      event_id: id,
      kind,
      observed_at_unix_seconds: Math.floor(Date.now() / 1000),
      funnel_id: funnelId,
      revision_id: revisionId,
      page_id: pageId,
    };
    if (formId) payload.form_id = formId;
    const campaign = campaignId();
    if (campaign) payload.campaign_id = campaign;

    try {
      void window.fetch(endpoint, {
        method: "POST",
        mode: "same-origin",
        credentials: "omit",
        referrerPolicy: "no-referrer",
        cache: "no-store",
        keepalive: true,
        headers: {
          "content-type": "application/json",
          "x-eezfunnel-interaction-consent": "granted",
        },
        body: JSON.stringify(payload),
      }).catch(() => {});
    } catch (_) {
      // Measurement must never affect page or form behavior.
    }
  }

  function recordPageView() {
    if (choice === "granted" && !pageViewed) {
      pageViewed = true;
      send("page_viewed", null);
    }
  }

  function recordFormView(form) {
    const formId = form.dataset.eezfunnelFormId;
    if (choice === "granted" && formId && !viewedForms.has(formId)) {
      viewedForms.add(formId);
      send("form_viewed", formId);
    }
  }

  function checkVisibleForms() {
    for (const form of formElements) {
      const bounds = form.getBoundingClientRect();
      if (
        bounds.bottom > 0 &&
        bounds.right > 0 &&
        bounds.top < window.innerHeight &&
        bounds.left < window.innerWidth
      ) {
        recordFormView(form);
      }
    }
  }

  let observer = null;
  if ("IntersectionObserver" in window) {
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) recordFormView(entry.target);
        }
      },
      { threshold: 0.1 },
    );
    for (const form of formElements) observer.observe(form);
  } else {
    window.addEventListener("scroll", checkVisibleForms, { passive: true });
    window.addEventListener("resize", checkVisibleForms, { passive: true });
  }

  for (const form of formElements) {
    form.addEventListener("input", recordFormStart);
    form.addEventListener("change", recordFormStart);
    form.addEventListener("focusin", recordFormStart);
  }

  function recordFormStart(event) {
    const form = event.currentTarget;
    const formId = form.dataset.eezfunnelFormId;
    if (choice === "granted" && formId && !startedForms.has(formId)) {
      startedForms.add(formId);
      send("form_started", formId);
    }
  }

  function saveChoice(nextChoice) {
    choice = nextChoice;
    try {
      window.localStorage.setItem(choiceKey, nextChoice);
    } catch (_) {
      // Keep this choice for the current page when browser storage is unavailable.
    }
    panel.hidden = true;
    if (nextChoice === "granted") {
      recordPageView();
      checkVisibleForms();
    }
    trigger.focus();
  }

  trigger.addEventListener("click", () => {
    panel.hidden = !panel.hidden;
    if (!panel.hidden) allow.focus();
  });
  allow.addEventListener("click", () => saveChoice("granted"));
  refuse.addEventListener("click", () => saveChoice("refused"));

  if (choice === "granted") {
    recordPageView();
    checkVisibleForms();
  }
})();
