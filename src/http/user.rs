use crate::http::{auth, auth::AuthContext, AppContext, Error};

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// SQL query used to create a new user.
const CREATE_USER_QUERY: &str =
    "INSERT INTO \"user\" (name, email, password) VALUES ($1, $2, $3) RETURNING *";

/// SQL query used to fetch a user by id.
const GET_USER_BY_ID_QUERY: &str = "SELECT * FROM \"user\" WHERE id = $1";

/// The [`UserRow`] struct is used to let the `sqlx` library easily map a row from the `user` table
/// in the databse to a struct value.
#[derive(Debug, FromRow)]
struct UserRow {
    /// Id of the user.
    id: Uuid,
    /// Name of the user.
    name: String,
    /// Email address of the user.
    email: String,
    /// Bio for the the user.
    bio: String,
    /// URL to the image of the user.
    image: Option<String>,
    /// Time the user was created.
    #[allow(dead_code)]
    created: DateTime<Utc>,
    /// Time the user was last modified.
    #[allow(dead_code)]
    updated: Option<DateTime<Utc>>,
}

/// Creates the [`Router`] for the HTTP endpoints that correspond to the user domain and requires
/// the [`AppContext`] to be the state type.
///
/// The following list enumerates the endpoints which are exposed by the `users` API.
///
/// * `GET /api/users` - Retrieves the data for the currently logged in user based on the JWT.
/// * `POST /api/users` - Allows a new user to register.
/// * `PUT /api/users` - Allows a user to update their information.
/// * `POST /api/users/login` - Allows a user to authenticate and retrieve a valid JWT.
pub(super) fn router() -> Router<AppContext> {
    Router::new()
        .route("/api/users", post(create_user))
        .route("/api/user", get(get_user))
}

/// The [`CreateUser`] struct contains the data received from the HTTP request to register a new
/// user.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateUser {
    /// Requested username for the new user.
    username: String,
    /// Requested email for the new user.
    email: String,
    /// Plain text password for the new user.
    password: String,
}

/// The [`User`] struct contains data that repesents a user of the application as well as a JWT
/// that allows the user to authenticate with the application.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct User {
    /// Username of the user.
    username: String,
    /// Email address of the user.
    email: String,
    /// JWT that allows the user to authenticate with the server.
    token: String,
    /// Bio for the the user.
    bio: String,
    /// URL to the image of the user.
    image: Option<String>,
}

impl User {
    fn from_row_with_token(user_row: UserRow, token: String) -> User {
        User {
            username: user_row.name,
            email: user_row.email,
            token,
            bio: user_row.bio,
            image: user_row.image,
        }
    }
}

/// The [`UserBody`] struct is the envelope in which different data for a user is returned to the
/// client based on the incoming request.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserBody<T> {
    /// User data contained in the envelope.
    user: T,
}

/// Handles the user registration API endpoint at `POST /api/users`.
///
/// # Request Body Format
///
/// ``` json
/// {
///   "user":{
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "password": "jakejake"
///   }
/// }
/// ```
///
/// # Field Validation
///
/// * `username` - required and must be unique across all users
/// * `email` - required and must be unique across all users
/// * `password` - required
///
/// Note that in a real application you would probably want to set some minimum requirements on
/// the password but not be too overbearing with your maximum requirements if you impose any.
///
/// # Response Body Format
///
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "token": "jwt.token.here",
///     "bio": "I work at statefarm",
///     "image": null
///   }
/// }
async fn create_user(
    ctx: State<AppContext>,
    Json(request): Json<UserBody<CreateUser>>,
) -> Result<Json<UserBody<User>>, Error> {
    let password_hash = auth::hash_password(request.user.password)
        .await
        .map_err(|_| Error::Internal)?;

    // TODO: handle unique constraints

    let user_row: UserRow = sqlx::query_as(CREATE_USER_QUERY)
        .bind(request.user.username)
        .bind(request.user.email)
        .bind(password_hash)
        .fetch_one(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from database: {}", e);
            Error::from(e)
        })?;

    let token =
        auth::mint_jwt(user_row.id, &ctx.config.signing_key).map_err(|_| Error::Internal)?;

    let user = User::from_row_with_token(user_row, token);

    Ok(Json(UserBody { user }))
}

/// Handles the get current user API endpoint at `GET /api/user`. The handler will read the id of
/// the user from the current authentication token and return the user details after verifying the
/// signature.
///
/// # Response Body Format
///
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "token": "jwt.token.here",
///     "bio": "I work at statefarm",
///     "image": null
///   }
/// }
async fn get_user(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
) -> Result<Json<UserBody<User>>, Error> {
    let user_row: UserRow = sqlx::query_as(GET_USER_BY_ID_QUERY)
        .bind(&auth_ctx.user_id)
        .fetch_one(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    // TODO: pass back token passed in or create new one? it is not clear from the spec.
    let token =
        auth::mint_jwt(user_row.id, &ctx.config.signing_key).map_err(|_| Error::Internal)?;

    let user = User::from_row_with_token(user_row, token);

    Ok(Json(UserBody { user }))
}
