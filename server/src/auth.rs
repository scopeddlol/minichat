use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::UserRow;
use crate::perms;
use crate::state::AppState;

pub const SESSION_DAYS: i64 = 30;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub tv: i64,
    pub exp: i64,
    pub iat: i64,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

pub fn issue_token(secret: &str, user_id: &str, token_version: i64) -> AppResult<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        tv: token_version,
        iat: now.timestamp(),
        exp: (now + Duration::days(SESSION_DAYS)).timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("token signing failed: {e}")))
}

pub fn decode_token(secret: &str, token: &str) -> AppResult<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized("Your session has expired. Please sign in again.".into()))
}

/// An authenticated request: the user plus their effective instance-wide
/// permission bits.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user: UserRow,
    pub permissions: i64,
    pub role_ids: Vec<String>,
}

impl Auth {
    pub fn id(&self) -> &str {
        &self.user.id
    }
    pub fn can(&self, perm: i64) -> bool {
        perms::has(self.permissions, perm)
    }
    pub fn require(&self, perm: i64) -> AppResult<()> {
        if self.can(perm) {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "You don't have permission to do that.".into(),
            ))
        }
    }
}

/// Resolve a bearer token into an `Auth`, folding in role permissions.
/// Operators always receive `ADMINISTRATOR` so an instance can never lock
/// its owner out by role misconfiguration.
pub async fn authenticate(state: &AppState, token: &str) -> AppResult<Auth> {
    let claims = decode_token(&state.config.jwt_secret, token)?;
    let user: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Account no longer exists.".into()))?;

    if user.token_version != claims.tv {
        return Err(AppError::Unauthorized(
            "Your session is no longer valid. Please sign in again.".into(),
        ));
    }
    if user.is_suspended {
        return Err(AppError::Forbidden(
            "Your account has been suspended on this instance.".into(),
        ));
    }

    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT r.id, r.permissions FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = ?",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;

    let mut permissions = rows.iter().fold(0i64, |acc, (_, p)| acc | p);
    let role_ids = rows.into_iter().map(|(id, _)| id).collect();
    if user.is_operator {
        permissions |= perms::ADMINISTRATOR;
    }

    Ok(Auth {
        user,
        permissions,
        role_ids,
    })
}

fn bearer(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer(parts)
            .ok_or_else(|| AppError::Unauthorized("You need to sign in first.".into()))?;
        authenticate(state, &token).await
    }
}
