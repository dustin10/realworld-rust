use axum::{routing::get, Json, Router};
use serde::Serialize;

/// Creates the [`Router`] that exposes the health check endpoint for the application.
pub fn router() -> Router {
    Router::new().route("/health", get(check_health))
}

/// Enumerates the supported states of a health check.
#[derive(Debug, Serialize)]
enum Status {
    Ok,
}

/// The [`Health`] struct contains the result of the health check for the application.
#[derive(Debug, Serialize)]
struct Health {
    /// Overall status of the health check
    status: Status,
}

/// Handles the health check API endpoint at `GET /health`. Health check endpoints are typically
/// used by load balancers to periodically determine whether the application instance is healthy
/// or needs to be replaced.
///
/// # Response Body Format
///
/// {
///   "status": "Ok"
/// }
async fn check_health() -> Json<Health> {
    // TODO: ping db and kafka infra
    Json(Health { status: Status::Ok })
}
