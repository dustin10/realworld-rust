mod article;
mod auth;
mod health;
mod profile;
mod tag;
mod user;

use crate::config::Config;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

/// The [`AppContext`] is the state that is shared between all HTTP handler functions and makes
/// common data and functionality available to them.
#[derive(Clone, Debug)]
pub struct AppContext {
    /// Configuration for the application.
    pub config: Arc<Config>,
    /// Connection pool that allows for querying the database.
    pub db: PgPool,
    /// Sender used to notify the outbox processor that an outbox entry has been created.
    pub outbox_tx: Sender<()>,
}

/// Creates the [`Router`] that exposes all of the routes that the application serves over HTTP.
pub fn router(db: PgPool, config: Arc<Config>, outbox_tx: Sender<()>) -> Router {
    let context = AppContext {
        config,
        db,
        outbox_tx,
    };

    let article_router = article::router().with_state(context.clone());
    let profile_router = profile::router().with_state(context.clone());
    let tag_router = tag::router().with_state(context.clone());
    let user_router = user::router().with_state(context);
    let health_router = health::router();

    article_router
        .merge(profile_router)
        .merge(tag_router)
        .merge(user_router)
        .merge(health_router)
}

/// Enumerates the possible error scenarios for the `http` module.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Occurs when the client has submitted a request that has invalid parameters in the payload.
    #[error("invalid data contained in request")]
    Validation,
    /// Occurs when an error is encountered in the database layer.
    #[error("error occurred at the database")]
    Database {
        #[from]
        source: sqlx::Error,
    },
    /// Occurs when there is an internal server error that cannot be recovered from.
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for Error {
    /// Converts an [`Error`] value into a valid [`Response`] that can be returned by the
    /// application if encountered.
    fn into_response(self) -> Response {
        match self {
            Error::Validation => StatusCode::UNPROCESSABLE_ENTITY.into_response(),
            Error::Database { source } => {
                tracing::error!("database error: {}", source);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Error::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

/// Returns the default size of a page of results. Used for deefining the default value during the
/// derserialization of the query parameters.
const fn default_limit() -> i32 {
    20
}

/// The [`Pagination`] struct contains data that informs an API how to select a page of results to
/// return to the client. The values are extracted out of the query parameters in the request.
#[derive(Debug, Deserialize)]
struct Pagination {
    /// Maximum number of results to return for a single request.
    #[serde(default = "default_limit")]
    limit: i32,
    /// Starting offset into the entire set of results.
    #[serde(default)]
    offset: i32,
}
