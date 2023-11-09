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
use sqlx::PgPool;
use std::sync::Arc;

/// The [`AppContext`] is the state that is shared between all HTTP handler functions and makes
/// common data and functionality available to them.
#[derive(Clone, Debug)]
pub struct AppContext {
    /// Configuration for the application.
    pub config: Arc<Config>,
    /// Connection pool that allows for querying the database.
    pub db: PgPool,
}

/// Creates the [`Router`] that exposes all of the routes that the application serves over HTTP.
pub fn router(db: PgPool, config: Config) -> Router {
    let context = AppContext {
        config: Arc::new(config),
        db,
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
            Error::Database { .. } => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Error::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
