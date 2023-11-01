use crate::http::{auth::AuthContext, AppContext, Error};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch a profile by the name of the user.
const GET_PROFILE_BY_USERNAME_QUERY: &str = r#"
    SELECT
        u.name,
        u.bio,
        u.image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS follows
    FROM
        users AS u
    WHERE
        u.name = $2"#;

/// SQL query used to fetch a profile by the id of the user.
const GET_PROFILE_BY_ID_QUERY: &str = r#"
    SELECT
        u.name,
        u.bio,
        u.image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS follows
    FROM
        users AS u
    WHERE
        u.id = $2"#;

/// SQL query which allows a user to follow a profile.
const INSERT_FOLLOW_QUERY: &str =
    "INSERT INTO user_follows (user_id, follower_id) VALUES ((SELECT u.id FROM users AS u WHERE u.name = $1), $2)";

/// SQL query which allows a user to unfollow a profile.
const DELETE_FOLLOW_QUERY: &str =
    "DELETE FROM user_follows AS uf WHERE uf.user_id = (SELECT u.id FROM users AS u WHERE u.name = $1) AND uf.follower_id = $2";

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
    Router::new()
        .route("/api/profiles/:username", get(get_profile))
        .route(
            "/api/profiles/:username/follow",
            post(follow_profile).delete(unfollow_profile),
        )
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
pub(crate) struct Profile {
    /// Username of the profile.
    #[serde(rename = "username")]
    pub name: String,
    /// Bio for the the profile.
    pub bio: String,
    /// URL to the image of the profile.
    pub image: Option<String>,
    /// Flag indicating whether or not the profile is being followed by the currently authenticated
    /// user. If no user is curently logged in, then the value will be set to `false`.
    pub follows: bool,
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
/// ``` json
/// {
///   "profile": {
///     "username": "jake",
///     "bio": "I work at statefarm",
///     "image": "https://api.realworld.io/images/smiley-cyrus.jpg",
///     "follows": false
///   }
/// }
/// ```
async fn get_profile(
    Path(username): Path<String>,
    ctx: State<AppContext>,
    auth_ctx: Option<AuthContext>,
) -> Result<Response, Error> {
    let auth_id = auth_ctx.map(|ac| ac.user_id);

    match fetch_profile_by_username(&ctx.db, &username, auth_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(profile).into_response()),
    }
}

/// Handles the follow user public profile API endpoint at `POST /api/profiles/:username/follow`.
/// The handler will read the `username` path parameter value, the `user_id` from the
/// [`AuthContext`] and use those values to create a record of the profile follow in the database.
///
/// # Response Body Format
///
/// ``` json
/// {
///   "profile": {
///     "username": "jake",
///     "bio": "I work at statefarm",
///     "image": "https://api.realworld.io/images/smiley-cyrus.jpg",
///     "follows": true
///   }
/// }
/// ```
async fn follow_profile(
    Path(username): Path<String>,
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
) -> Result<Response, Error> {
    match insert_follow(&ctx.db, &username, auth_ctx.user_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(profile).into_response()),
    }
}

/// Handles the unfollow user public profile API endpoint at `POST /api/profiles/:username/unfollow`.
/// The handler will read the `username` path parameter value, the `user_id` from the [`AuthContext`]
/// and use those values to delete the record of the profile follow from the database.
///
/// # Response Body Format
///
/// ``` json
/// {
///   "profile": {
///     "username": "jake",
///     "bio": "I work at statefarm",
///     "image": "https://api.realworld.io/images/smiley-cyrus.jpg",
///     "follows": false
///   }
/// }
/// ```
async fn unfollow_profile(
    Path(username): Path<String>,
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
) -> Result<Response, Error> {
    match delete_follow(&ctx.db, &username, auth_ctx.user_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(profile).into_response()),
    }
}

/// Retrieves a [`Profile`] from the database given the name of the user that the profile
/// represents and the id of the authenticated user if available.
async fn fetch_profile_by_username(
    db: &PgPool,
    username: &str,
    auth_id: Option<Uuid>,
) -> Result<Option<Profile>, Error> {
    let user_context = auth_id.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_PROFILE_BY_USERNAME_QUERY)
        .bind(user_context)
        .bind(username)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Retrieves a [`Profile`] from the database given the id of the user that the profile
/// represents and the id of the authenticated user if available.
pub(crate) async fn fetch_profile_by_id(
    db: &PgPool,
    id: &Uuid,
    auth_id: Option<Uuid>,
) -> Result<Option<Profile>, Error> {
    let user_context = auth_id.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_PROFILE_BY_ID_QUERY)
        .bind(user_context)
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Inserts an entry into the table that tracks profile follows for a users. Returns the updated
/// [`Profile`] for the user that was followed in the context of the authenticated user.
async fn insert_follow(
    db: &PgPool,
    username: &str,
    follower_id: Uuid,
) -> Result<Option<Profile>, Error> {
    let _ = sqlx::query(INSERT_FOLLOW_QUERY)
        .bind(username)
        .bind(follower_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    fetch_profile_by_username(db, username, Some(follower_id)).await
}

/// Deletes an entry from the table that tracks profile follows for a users. Returns the updated
/// [`Profile`] for the user that was unfollowed in the context of the authenticated user.
async fn delete_follow(
    db: &PgPool,
    username: &str,
    follower_id: Uuid,
) -> Result<Option<Profile>, Error> {
    let _ = sqlx::query(DELETE_FOLLOW_QUERY)
        .bind(username)
        .bind(follower_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    fetch_profile_by_username(db, username, Some(follower_id)).await
}
