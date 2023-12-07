use crate::{
    db,
    http::{auth, auth::AuthContext, AppContext, Error},
};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};

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
        .route("/api/users/login", post(login_user))
        .route("/api/users", post(create_user))
        .route("/api/user", get(get_user).put(update_user))
}

/// The [`CreateUserRequest`] struct contains the data received from the HTTP request to register a new
/// user.
#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    /// Requested username for the new user.
    username: String,
    /// Requested email for the new user.
    email: String,
    /// Plain text password for the new user.
    password: String,
}

/// The [`LoginUserRequest`] struct contains the data received from the HTTP request to authenticate a
/// user.
#[derive(Debug, Deserialize)]
struct LoginUserRequest {
    /// Email for the user.
    email: String,
    /// Plain text password for the user.
    password: String,
}

/// The [`UpdateUserRequest`] struct contains the data received from the HTTP request to update a user.
#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    /// Username of the user.
    username: Option<String>,
    /// Email address of the user.
    email: Option<String>,
    /// Plain text password for the user.
    password: Option<String>,
    /// Bio for the the user.
    bio: Option<String>,
    /// URL to the image of the user.
    image: Option<String>,
}

/// The [`User`] struct contains data that repesents a user of the application as well as a JWT
/// that allows the user to authenticate with the application.
#[derive(Debug, Deserialize, Serialize)]
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
    /// Creates a new [`User`] from the given [`db::user::User`] retrieved from the database and the
    /// specified authentication token.
    fn from_db_user_with_token(user: db::user::User, token: String) -> User {
        User {
            username: user.name,
            email: user.email,
            token,
            bio: user.bio,
            image: user.image,
        }
    }
}

/// The [`UserBody`] struct is the envelope in which different data for a user is returned to the
/// client based on the incoming request.
#[derive(Debug, Deserialize, Serialize)]
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
/// ```json
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "token": "jwt.token.here",
///     "bio": "I work at statefarm",
///     "image": null
///   }
/// }
/// ```
async fn create_user(
    ctx: State<AppContext>,
    Json(request): Json<UserBody<CreateUserRequest>>,
) -> Result<Json<UserBody<User>>, Error> {
    let password_hash = auth::hash_password(request.user.password)
        .await
        .map_err(|e| {
            tracing::error!("error hashing password: {}", e);
            Error::Internal
        })?;

    let data = db::user::CreateUser {
        username: &request.user.username,
        email: &request.user.email,
        hashed_password: &password_hash,
    };

    // TODO: handle unique constraints

    let db_user: db::user::User = db::user::create_user(&ctx.db, data).await?;

    let token = auth::mint_jwt(db_user.id, &ctx.config.signing_key).map_err(|e| {
        tracing::error!("error minting jwt: {}", e);
        Error::Internal
    })?;

    let user = User::from_db_user_with_token(db_user, token);

    Ok(Json(UserBody { user }))
}

/// Handles the user authentication API endpoint at `GET /api/users/login`.
///
/// # Request Body Format
///
/// ``` json
/// {
///   "user":{
///     "email": "jake@jake.jake",
///     "password": "jakejake"
///   }
/// }
/// ```
///
/// # Required Fields
///
/// * `email`
/// * `password`
///
/// # Response Body Format
///
/// ``` json
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "token": "jwt.token.here",
///     "bio": "I work at statefarm",
///     "image": null
///   }
/// }
/// ```
async fn login_user(
    ctx: State<AppContext>,
    Json(request): Json<UserBody<LoginUserRequest>>,
) -> Result<Response, Error> {
    // if no user is found then just return UNAUTHORIZED instead of not found to prevent an
    // attacker from fishing for valid email addresses
    match db::user::fetch_user_by_email(&ctx.db, &request.user.email).await? {
        None => Ok(StatusCode::UNAUTHORIZED.into_response()),
        Some(db_user) => {
            let resp = if auth::verify_password(request.user.password, db_user.password.clone())
                .await
            {
                let token = auth::mint_jwt(db_user.id, &ctx.config.signing_key).map_err(|e| {
                    tracing::error!("error minting jwt: {}", e);
                    Error::Internal
                })?;

                let user = User::from_db_user_with_token(db_user, token);

                Json(UserBody { user }).into_response()
            } else {
                tracing::debug!("password verification failed for {}", request.user.email);
                StatusCode::UNAUTHORIZED.into_response()
            };

            Ok(resp)
        }
    }
}

/// Handles the get current user API endpoint at `GET /api/user`. The handler will read the id of
/// the user from the current authentication token and return the user details after verifying the
/// signature.
///
/// # Response Body Format
///
/// ``` json
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.jake",
///     "token": "jwt.token.here",
///     "bio": "I work at statefarm",
///     "image": null
///   }
/// }
/// ```
async fn get_user(ctx: State<AppContext>, auth_ctx: AuthContext) -> Result<Response, Error> {
    match db::user::fetch_user_by_id(&ctx.db, &auth_ctx.user_id).await? {
        Some(db_user) => {
            let user = User::from_db_user_with_token(db_user, auth_ctx.encoded_jwt);

            Ok(Json(UserBody { user }).into_response())
        }
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

/// Handles the update user API endpoint at `PUT /api/users`. The handler will read the id of the
/// user from the current authentication token and update the user properties based on the request
/// body.
///
/// # Request Body Format
///
/// ``` json
/// {
///   "user":{
///     "email": "jake@jake.com",
///     "bio": "I like to skateboard",
///     "image": "https://i.stack.imgur.com/xHWG8.jpg"
///   }
/// }
/// ```
///
/// # Accepted Fields
///
/// * `email`
/// * `username`
/// * `password`
/// * `image`
/// * `bio`
///
/// # Response Body Format
///
/// ``` json
/// {
///   "user": {
///     "username": "jake",
///     "email": "jake@jake.com",
///     "token": "jwt.token.here",
///     "bio": "I like to skateboard",
///     "image": "https://i.stack.imgur.com/xHWG8.jpg"
///   }
/// }
/// ```
async fn update_user(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Json(request): Json<UserBody<UpdateUserRequest>>,
) -> Result<Response, Error> {
    match db::user::fetch_user_by_id(&ctx.db, &auth_ctx.user_id).await? {
        None => Ok(StatusCode::UNAUTHORIZED.into_response()),
        Some(db_user) => {
            let username = request.user.username.as_ref().unwrap_or(&db_user.name);
            let email = request.user.email.as_ref().unwrap_or(&db_user.email);
            let bio = request.user.bio.as_ref().unwrap_or(&db_user.bio);
            let image = request.user.image.or(db_user.image);

            let password_hash = if let Some(password) = request.user.password {
                auth::hash_password(password).await.map_err(|e| {
                    tracing::error!("error hashing password: {}", e);
                    Error::Internal
                })?
            } else {
                db_user.password
            };

            let data = db::user::UpdateUser {
                id: &db_user.id,
                username,
                email,
                bio,
                image: image.as_ref(),
                hashed_password: &password_hash,
            };

            // TODO: handle unique constraint violations

            let db_user: db::user::User = db::user::update_user(&ctx.db, data).await?;

            // TODO: if password changes should a new token be minted?

            let user = User::from_db_user_with_token(db_user, auth_ctx.encoded_jwt);

            Ok(Json(UserBody { user }).into_response())
        }
    }
}
