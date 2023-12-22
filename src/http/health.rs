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
    #[allow(dead_code)]
    Warn,
    #[allow(dead_code)]
    Error,
}

/// The [`Health`] struct contains the result of the health check for the application.
#[derive(Debug, Serialize)]
struct Health {
    /// Overall status of the application health check.
    status: Status,
    /// Status of all the health checks performed.
    checks: Vec<HealthCheck>,
}

/// The [`HealthCheck`] struct contains the result of an individual health check which typically
/// targets a specific piece of infrastructure that the application relies on to function properly.
#[derive(Debug, Serialize)]
struct HealthCheck {
    /// Name of the health check module.
    name: String,
    /// Status of the health check module.
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
    // TODO: ping db and kafka infra and probably just warn if unable to connect?
    Json(Health {
        status: Status::Ok,
        checks: Vec::new(),
    })
}
