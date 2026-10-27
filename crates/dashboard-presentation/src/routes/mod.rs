use dioxus::prelude::*;

mod api;
mod browser;
mod editor;
mod funnels;
mod idempotency;
mod reports;
mod submissions;

use editor::FunnelDetail;
use funnels::Funnels;
use reports::{FunnelCompletions, FunnelInteractionReport};
use submissions::Submissions;

#[derive(Clone, PartialEq, Routable)]
#[rustfmt::skip]
pub(crate) enum RouteView {
    #[layout(DashboardShell)]
    #[route("/")]
    Dashboard {},
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels")]
    Funnels {},
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels/:id")]
    FunnelDetail { id: String },
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels/:id/completions")]
    FunnelCompletions { id: String },
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels/:id/interaction-report")]
    FunnelInteractionReport { id: String },
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/submissions")]
    Submissions {},
}

#[component]
fn DashboardShell() -> Element {
    let breadcrumb = match use_route::<RouteView>() {
        RouteView::Dashboard {} => "Workspace / Overview",
        RouteView::Funnels {} => "Workspace / Funnels",
        RouteView::FunnelDetail { .. } => "Workspace / Funnel editor",
        RouteView::FunnelCompletions { .. } => "Workspace / Form completions",
        RouteView::FunnelInteractionReport { .. } => "Workspace / Interaction counts",
        RouteView::Submissions {} => "Workspace / Submissions",
    };
    rsx! {
        div { class: "app-shell",
            aside { class: "sidebar",
                Link { to: RouteView::Dashboard {}, class: "brand",
                    span { class: "brand-mark", "E" }
                    span { "Eez Funnel" }
                }
                div { class: "sidebar-section",
                    p { class: "nav-label", "Workspace" }
                    nav { class: "primary-nav", "aria-label": "Main navigation",
                        Link { to: RouteView::Dashboard {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "m3 10 9-7 9 7 M5 9v11h14V9 M9 20v-6h6v6" }
                                }
                            }
                            span { "Dashboard" }
                        }
                        Link { to: RouteView::Funnels {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M4 4h16v16H4z M8 8h8 M8 12h8 M8 16h5" }
                                }
                            }
                            span { "Funnels" }
                        }
                        Link { to: RouteView::Submissions {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M4 5h16v14H4z M4 12h4l2 3h4l2-3h4" }
                                }
                            }
                            span { "Submissions" }
                        }
                    }
                }
            }
            div { class: "workspace",
                header { class: "topbar",
                    span { class: "breadcrumb", "{breadcrumb}" }
                    div { class: "owner-chip",
                        span { class: "owner-avatar", "EF" }
                        span { "Funnel workspace" }
                    }
                }
                main { class: "page-content",
                    Outlet::<RouteView> {}
                }
            }
        }
    }
}

#[component]
fn Dashboard() -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Overview" }
            h1 { "Dashboard" }
            p { class: "page-description", "A clear view of your funnel workspace." }
        }
        section { class: "welcome-card", "aria-label": "Welcome to your dashboard",
            div {
                p { class: "eyebrow", "Your workspace" }
                h2 { "Build a smoother customer journey" }
                p { "Your dashboard is ready. Your funnels and workspace tools will appear here as you grow." }
            }
            div { class: "welcome-art", "aria-hidden": "true",
                div { class: "art-orbit",
                    div { class: "art-card",
                        div { class: "art-line" }
                        div { class: "art-line" }
                        div { class: "art-line" }
                    }
                }
            }
        }
        div { class: "section-heading",
            h2 { "Workspace snapshot" }
            span { "In development" }
        }
        section { class: "empty-state", "aria-label": "Workspace summary",
            "Workspace activity will appear here when its data is available."
        }
    }
}
