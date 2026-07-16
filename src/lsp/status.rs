//! Classification of rust-analyzer's `experimental/serverStatus` notifications.
//!
//! rust-analyzer sends `experimental/serverStatus` only when the client
//! advertises `experimental.serverStatusNotification: true` at `initialize`
//! (verified: a client without the advert receives zero such notifications).
//! The notification carries `{ health, quiescent, message }`; `quiescent:
//! true` means the server has no pending work — the earliest point at which
//! definition queries stop returning empty results or `-32801 content
//! modified` (see `.tethys-2mjj/findings.md` for the probed timelines).

use serde_json::Value;

/// The rust-analyzer extension notification method carrying server status.
///
/// Colocated with the classifier so the method the transport matches on
/// and the params shape classified here cannot drift apart.
pub(crate) const SERVER_STATUS_METHOD: &str = "experimental/serverStatus";

/// Readiness classification of one `experimental/serverStatus` notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadyState {
    /// The server reports no pending work (`quiescent: true`).
    Ready {
        /// True when `health` is present and not `"ok"`: the server is idle
        /// but could not fully load the workspace (e.g. no Cargo project
        /// discovered). Queries are answerable but may legitimately return
        /// nothing; callers log this and proceed.
        degraded: bool,
    },
    /// The server still has pending work — or the params were malformed, in
    /// which case a drain loop keeps waiting rather than trusting garbage.
    NotReady,
}

/// Classify the params of an `experimental/serverStatus` notification.
///
/// `quiescent` alone governs readiness; a missing `health` field is treated
/// as healthy. Missing or non-boolean `quiescent` classifies as
/// [`ReadyState::NotReady`] so malformed notifications can never end a
/// readiness wait early (the wait's timeout is the backstop).
pub(crate) fn classify_server_status(params: &Value) -> ReadyState {
    match params.get("quiescent").and_then(Value::as_bool) {
        Some(true) => {
            let degraded = params
                .get("health")
                .and_then(Value::as_str)
                .is_some_and(|health| health != "ok");
            ReadyState::Ready { degraded }
        }
        Some(false) | None => ReadyState::NotReady,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verbatim capture from `.tethys-2mjj/probe-cold.log` at t=0.06s — the
    /// first notification rust-analyzer sends, before any load completes.
    #[test]
    fn ready_classifier_not_quiescent() {
        let params = json!({"health": "ok", "quiescent": false, "message": null});
        assert_eq!(classify_server_status(&params), ReadyState::NotReady);
    }

    /// Verbatim capture from `.tethys-2mjj/probe-cold.log` at t=8.70s — the
    /// load-complete notification that coincided with queries answering.
    #[test]
    fn ready_classifier_quiescent_ok() {
        let params = json!({"health": "ok", "quiescent": true, "message": null});
        assert_eq!(
            classify_server_status(&params),
            ReadyState::Ready { degraded: false }
        );
    }

    /// Verbatim captures from `probe-edge.py` on a workspace with no Cargo
    /// project: quiescent immediately, health `warning` then `error`.
    #[test]
    fn ready_classifier_quiescent_degraded() {
        for health in ["warning", "error"] {
            let params = json!({
                "health": health,
                "quiescent": true,
                "message": "Failed to discover workspace."
            });
            assert_eq!(
                classify_server_status(&params),
                ReadyState::Ready { degraded: true },
                "health={health} with quiescent=true must classify Ready(degraded)"
            );
        }
    }

    /// `quiescent` governs readiness on its own: a notification without
    /// `health` is Ready and not degraded.
    #[test]
    fn ready_classifier_missing_health_is_healthy() {
        let params = json!({"quiescent": true});
        assert_eq!(
            classify_server_status(&params),
            ReadyState::Ready { degraded: false }
        );
    }

    /// Malformed shapes keep the wait going: missing `quiescent`, a string
    /// `"true"` (no loose truthiness), and non-object params must all
    /// classify `NotReady` — never panic, never end the wait early.
    #[test]
    fn ready_classifier_malformed_params() {
        for params in [
            json!({"health": "ok"}),
            json!({"health": "ok", "quiescent": "true"}),
            json!({"health": "ok", "quiescent": 1}),
            Value::Null,
            json!([]),
        ] {
            assert_eq!(
                classify_server_status(&params),
                ReadyState::NotReady,
                "malformed params {params} must classify NotReady"
            );
        }
    }
}
