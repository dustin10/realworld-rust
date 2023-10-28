use crate::http::AppContext;

use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use async_trait::async_trait;
use axum::{extract::FromRequestParts, http::StatusCode};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use http::request::Parts;
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::Duration;
use uuid::Uuid;

/// Name of the header that contains the authorization JWT
const AUTH_HEADER: &str = "authorization";

/// Prefix of the auth header value before the JWT begins, i.e. `Token <jwt-here>`
const AUTH_PREFIX: &str = "Token ";

/// Enumerates the possible error states for the `auth` module.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Occurs when an error is encountered trying to calculate the hash of a password.
    #[error("error hashing password")]
    Hash,
    /// Occurs when an error is encountered trying to sign the authentication token.
    #[error("error signing authentication token")]
    Signing,
    /// Occurs when an error is encountered trying to verify the authentication token.
    #[error("error verifying authentication token")]
    Verification,
}

/// The [`AuthContext`] contains the authorization context for the current request. The data is
/// extracted from the authorization JWT specified in the HTTP request header.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthContext {
    /// Id of the authenticated user.
    pub user_id: Uuid,
    /// Encoded authentication token that the [`AuthContext`] was derived from.
    pub encoded_jwt: String,
}

#[async_trait]
impl FromRequestParts<AppContext> for AuthContext {
    type Rejection = StatusCode;

    /// Bootstraps an [`AuthContext`] using the encoded token contained in the HTTP header value.
    /// If the header does not exist then an [`Err`] containing a [`StatusCode::UNAUTHORIZED`] will
    /// be returned.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppContext,
    ) -> Result<Self, Self::Rejection> {
        // Extract the Authorization header as defined in the application spec and then verify the JWT
        // contained in the header value.
        //
        // Most applications would use the Bearer prefix rather than Token, so axum has some
        // built-in types to help, e.g. TypedHeader::<Authorization<Bearer>>::from_request_parts,
        // but here we just parse the header value ourselves.
        match parts
            .headers
            .get(AUTH_HEADER)
            .and_then(|hv| hv.to_str().ok())
        {
            Some(hdr) => {
                let jwt = &hdr[AUTH_PREFIX.len()..];

                verify_jwt(jwt, &state.config.signing_key).map_err(|e| {
                    tracing::error!("error verifying JWT: {}", e);
                    StatusCode::UNAUTHORIZED
                })
            }
            None => {
                tracing::debug!("no authorization header found");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

/// The [`Claims`] struct represents the data contained in the claims section of the JWT.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Claims {
    /// Id of the authenticated user.
    user_id: Uuid,
    /// Time of token expiry.
    #[serde(rename = "exp")]
    expires_at: DateTime<Utc>,
}

/// Creates a new authentication token for a user signed with the specified key.
pub fn mint_jwt(user_id: Uuid, signing_key: &str) -> Result<String, Error> {
    let hmac: Hmac<Sha256> = Hmac::new_from_slice(signing_key.as_bytes()).map_err(|e| {
        tracing::debug!("error creating jwt signing key: {}", e);
        Error::Signing
    })?;

    let claims = Claims {
        user_id,
        expires_at: Utc::now() + Duration::from_secs(3600),
    };

    claims.sign_with_key(&hmac).map_err(|e| {
        tracing::debug!("error signing jwt: {}", e);
        Error::Signing
    })
}

/// Authenticates the encoded JWT by verifying the signature and ensuring it is not expired,
/// then bootstraps an [`AuthContext`] with the data contained in the verified token.
pub fn verify_jwt(encoded_jwt: &str, signing_key: &str) -> Result<AuthContext, Error> {
    let hmac: Hmac<Sha256> = Hmac::new_from_slice(signing_key.as_bytes()).map_err(|e| {
        tracing::debug!("error creating jwt signing key: {}", e);
        Error::Verification
    })?;

    let claims: Claims = encoded_jwt.verify_with_key(&hmac).map_err(|e| {
        tracing::debug!("error verifying jwt: {}", e);
        Error::Verification
    })?;

    if claims.expires_at < Utc::now() {
        tracing::debug!("rejecting JWT as it is expired");
        return Err(Error::Verification);
    }

    Ok(AuthContext {
        user_id: claims.user_id,
        encoded_jwt: encoded_jwt.to_owned(),
    })
}

/// Hashes the given plain-text passsword.
///
/// The hashing operation is very CPU intensive so spawn a task to be run in the rayon thread
/// pool which is good for CPU that kind of work.
pub async fn hash_password(password: String) -> Result<String, Error> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    rayon::spawn(move || {
        let salt = SaltString::generate(&mut OsRng);

        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|ph| ph.to_string())
            .map_err(|e| {
                tracing::debug!("error hashing password: {}", e);
                Error::Hash
            });

        if tx.send(password_hash).is_err() {
            tracing::error!("failed to send password hash result over channel");
        }
    });

    let hash = rx.await.map_err(|e| {
        tracing::debug!("error hashing password: {}", e);
        Error::Hash
    })??;

    Ok(hash)
}

/// Verifies the password hash for the given password. A value of `false` will be returned
/// if any error is encountered during verification.
///
/// The hash verification operation is very CPU intensive so spawn a task to be run in the
/// rayon thread pool which is good for that kind of work.
pub async fn verify_password(password: String, password_hash: String) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();

    rayon::spawn(move || {
        let verified = match PasswordHash::new(&password_hash) {
            Ok(parsed) => Argon2::default()
                .verify_password(password.as_ref(), &parsed)
                .is_ok(),
            Err(e) => {
                tracing::debug!("failed to parse hashed password: {}", e);
                false
            }
        };

        if tx.send(verified).is_err() {
            tracing::error!("failed to send password verification result over channel");
        }
    });

    rx.await.unwrap_or_else(|e| {
        tracing::debug!("error verifying password: {}", e);
        false
    })
}
