use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

/// SQL query used to create a new user.
const CREATE_USER_QUERY: &str =
    "INSERT INTO users (name, email, password) VALUES ($1, $2, $3) RETURNING *";

/// SQL query used to fetch a user by id.
const GET_USER_BY_ID_QUERY: &str = "SELECT * FROM users WHERE id = $1";

/// SQL query used to fetch a user by email.
const GET_USER_BY_EMAIL_QUERY: &str = "SELECT * FROM users WHERE email = $1";

/// SQL query used to update a user by id.
const UPDATE_USER_BY_ID_QUERY: &str =
    "UPDATE users SET name = $1, email = $2, password = $3, image = $4, bio = $5 WHERE id = $6 RETURNING *";

/// SQL query used to fetch a profile by the name of the user.
const GET_PROFILE_BY_USERNAME_QUERY: &str = r#"
    SELECT
        u.id,
        u.name,
        u.bio,
        u.image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS following
    FROM
        users AS u
    WHERE
        u.name = $2"#;

/// SQL query used to fetch a profile by the id of the user.
const GET_PROFILE_BY_ID_QUERY: &str = r#"
    SELECT
        u.id,
        u.name,
        u.bio,
        u.image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS following
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

/// The [`User`] struct is used to let the `sqlx` library easily map a row from the `users` table
/// in the database to a struct value.
#[derive(Debug, FromRow)]
pub struct User {
    /// Id of the user.
    pub id: Uuid,
    /// Name of the user.
    pub name: String,
    /// Email address of the user.
    pub email: String,
    /// Hashed password for the user.
    pub password: String,
    /// Bio for the the user.
    pub bio: String,
    /// URL to the image of the user.
    pub image: Option<String>,
    /// Time the user was created.
    #[allow(dead_code)]
    pub created: DateTime<Utc>,
    /// Time the user was last modified.
    #[allow(dead_code)]
    pub updated: Option<DateTime<Utc>>,
}

/// The [`CreateUser`] struct contains the data to used to create the database row representing a
/// user.
#[derive(Debug)]
pub struct CreateUser<'a> {
    /// Username of the new user.
    pub username: &'a String,
    /// Email address of the new user.
    pub email: &'a String,
    /// Hashed password for the new user.
    pub hashed_password: &'a String,
}

/// The [`UpdateUser`] struct contains the data to update the database row representing a user
/// with.
#[derive(Debug)]
pub struct UpdateUser<'a> {
    /// Id of the user.
    pub id: &'a Uuid,
    /// Username of the user.
    pub username: &'a String,
    /// Email address of the user.
    pub email: &'a String,
    /// Hashed password for the user.
    pub hashed_password: &'a String,
    /// Bio for the the user.
    pub bio: &'a String,
    /// URL to the image of the user.
    pub image: Option<&'a String>,
}

/// The [`Profile`] struct is used to let the `sqlx` library easily map the projection of a user
/// profile to a struct value.
#[derive(Debug, Deserialize, Serialize, FromRow)]
pub struct Profile {
    /// Id of the user the profile represents.
    #[serde(skip_serializing)]
    pub id: Uuid,
    /// Username of the profile.
    #[serde(rename = "username")]
    pub name: String,
    /// Bio for the the profile.
    pub bio: String,
    /// URL to the image of the profile.
    pub image: Option<String>,
    /// Flag indicating whether or not the profile is being followed by the currently authenticated
    /// user. If no user is curently logged in, then the value will be set to `false`.
    pub following: bool,
}

/// Retrieves a [`User`] from the database given the id of the user.
pub async fn query_user_by_id(
    cxn: &mut PgConnection,
    id: &Uuid,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(GET_USER_BY_ID_QUERY)
        .bind(id)
        .fetch_optional(cxn)
        .await
}

/// Retrieves a [`User`] from the database given the email address of the user.
pub async fn query_user_by_email(
    cxn: &mut PgConnection,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(GET_USER_BY_EMAIL_QUERY)
        .bind(email)
        .fetch_optional(cxn)
        .await
}

/// Creates a new [`User`] row in the database using the details contained in the given [`CreateUser`].
pub async fn create_user(
    cxn: &mut PgConnection,
    data: CreateUser<'_>,
) -> Result<User, sqlx::Error> {
    sqlx::query_as(CREATE_USER_QUERY)
        .bind(data.username)
        .bind(data.email)
        .bind(data.hashed_password)
        .fetch_one(cxn)
        .await
}

/// Updates a [`User`] row in the database using the details contained in the given [`UpdateUser`].
pub async fn update_user(
    cxn: &mut PgConnection,
    data: UpdateUser<'_>,
) -> Result<User, sqlx::Error> {
    sqlx::query_as(UPDATE_USER_BY_ID_QUERY)
        .bind(data.username)
        .bind(data.email)
        .bind(data.hashed_password)
        .bind(data.image)
        .bind(data.bio)
        .bind(data.id)
        .fetch_one(cxn)
        .await
}

/// Retrieves a [`Profile`] from the database given the name of the user that the profile
/// represents and the id of the authenticated user if available to determine the follower context.
pub async fn query_profile_by_username(
    cxn: &mut PgConnection,
    username: &str,
    user_ctx: Option<Uuid>, // TODO: property should be Option<&Uuid> instead
) -> Result<Option<Profile>, sqlx::Error> {
    let user_context = user_ctx.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_PROFILE_BY_USERNAME_QUERY)
        .bind(user_context)
        .bind(username)
        .fetch_optional(cxn)
        .await
}

/// Retrieves a [`Profile`] from the database given the id of the user that the profile
/// represents and the id of the authenticated user if available to determine the follower context.
pub async fn query_profile_by_id(
    cxn: &mut PgConnection,
    id: &Uuid,
    user_ctx: Option<Uuid>, // TODO: property should be Option<&Uuid> instead
) -> Result<Option<Profile>, sqlx::Error> {
    let user_context = user_ctx.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_PROFILE_BY_ID_QUERY)
        .bind(user_context)
        .bind(id)
        .fetch_optional(cxn)
        .await
}

/// Inserts an entry into the table that tracks profile follows for a user.
pub async fn add_profile_follow(
    cxn: &mut PgConnection,
    username: &str,
    follower_id: Uuid,
) -> Result<Option<Profile>, sqlx::Error> {
    let _ = sqlx::query(INSERT_FOLLOW_QUERY)
        .bind(username)
        .bind(follower_id)
        .execute(&mut *cxn)
        .await?;

    query_profile_by_username(cxn, username, Some(follower_id)).await
}

/// Deletes an entry from the table that tracks profile follows for a user.
pub async fn remove_profile_follow(
    cxn: &mut PgConnection,
    username: &str,
    follower_id: Uuid,
) -> Result<Option<Profile>, sqlx::Error> {
    let _ = sqlx::query(DELETE_FOLLOW_QUERY)
        .bind(username)
        .bind(follower_id)
        .execute(&mut *cxn)
        .await?;

    query_profile_by_username(cxn, username, Some(follower_id)).await
}
