use crate::http::{auth::AuthContext, AppContext, Error};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch a profile.
const GET_PROFILE_QUERY: &str = r#"
    SELECT
        u.name,
        u.bio,
        u.image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS follows
    FROM
        users AS u
    WHERE
        u.name = $2"#;

/// Creates the [`Router`] for the HTTP endpoints that correspond to the `profile` domain and requires
/// the [`AppContext`] to be the state type.
///
/// The following list enumerates the endpoints which are exposed by the `profile` API.
///
/// * `GET /api/profiles/:username` - Retrieves the public profile for a user identified by
/// `:username` and whether or not the authenticated user, if available, is following them.
/// * `POST /api/profiles/:username/follow` - Follows the user identified by `:username`.
/// * `DELETE /api/profiles/:username/follow` - Unfollows the user identified by `:username`.
pub(super) fn router() -> Router<AppContext> {
    Router::new().route("/api/profiles/:username", get(get_profile))
}

/// The [`ProfileBody`] struct is the envelope in which the [`Profile`] for a user is returned to the
/// client based on the incoming request.
#[derive(Debug, Deserialize, Serialize)]
struct ProfileBody {
    /// Public profile for a user of the application.
    profile: Profile,
}

/// The [`Profile`] struct contains the details of the public profile for a user of the
/// application.
#[derive(Debug, Deserialize, Serialize, FromRow)]
struct Profile {
    /// Username of the profile.
    #[serde(rename = "username")]
    name: String,
    /// Bio for the the profile.
    bio: String,
    /// URL to the image of the profile.
    image: Option<String>,
    /// Flag indicating whether or not the profile is being followed by the currently authenticated
    /// user. If no user is curently logged in, then the value will be set to `false`.
    follows: bool,
}

/// Handles the get user public profile API endpoint at `GET /api/profiles/:username`. The handler
/// will read the `username` path parameter value and return the profile data for the matching user
/// if it exists.
///
/// If the request is authenticated, then the `follows` property of the response will indicate
/// whether the currently authenticated user is following the profile. If the request is made
/// unauthenticated, then the `follows` property will still exists but always be set to `false`.
///
/// # Response Body Format
///
/// {
///   "profile": {
///     "username": "jake",
///     "bio": "I work at statefarm",
///     "image": "https://api.realworld.io/images/smiley-cyrus.jpg",
///     "follows": false
///   }
/// }
async fn get_profile(
    Path(username): Path<String>,
    ctx: State<AppContext>,
    auth_ctx: Option<AuthContext>,
) -> Result<Response, Error> {
    let auth_id = auth_ctx.map(|ac| ac.user_id);

    match fetch_profile(&ctx.db, &username, auth_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(profile).into_response()),
    }
}

/// Retrieves a [`Profile`] from the database given the name of the user that the profile
/// represents and the id of the authenticated user if available.
async fn fetch_profile(
    db: &PgPool,
    username: &str,
    auth_id: Option<Uuid>,
) -> Result<Option<Profile>, Error> {
    let follower_id = auth_id.unwrap_or_else(|| Uuid::nil());

    sqlx::query_as(GET_PROFILE_QUERY)
        .bind(follower_id)
        .bind(username)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}
