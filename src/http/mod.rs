use axum::{routing::get, Router};

/// Creates the [`Router`] that exposes all of the routes that the application serves over HTTP.
pub fn router() -> Router {
    Router::new().route("/ping", get(ping))
}

/// Simple temporary handler for the /ping route that simply replies with `pong`.
pub async fn ping() -> &'static str {
    "pong"
}
