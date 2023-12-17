use crate::{
    db,
    db::user::Profile,
    http::{auth::AuthContext, AppContext, Error},
};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};

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

    let mut tx = ctx.db.begin().await?;

    let response = match db::user::query_profile_by_username(&mut tx, &username, auth_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(ProfileBody { profile }).into_response()),
    };

    tx.commit().await?;

    response
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
    let mut tx = ctx.db.begin().await?;

    let response = match db::user::add_profile_follow(&mut tx, &username, auth_ctx.user_id).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(profile) => Ok(Json(ProfileBody { profile }).into_response()),
    };

    tx.commit().await?;

    response
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
    let mut tx = ctx.db.begin().await?;

    let response =
        match db::user::remove_profile_follow(&mut tx, &username, auth_ctx.user_id).await? {
            None => Ok(StatusCode::NOT_FOUND.into_response()),
            Some(profile) => Ok(Json(ProfileBody { profile }).into_response()),
        };

    tx.commit().await?;

    response
}
