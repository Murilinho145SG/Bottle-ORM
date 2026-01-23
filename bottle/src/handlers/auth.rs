use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use bcrypt::{hash, DEFAULT_COST};
use bottle_orm::{Pagination, Transaction};
use chrono::{DateTime, Utc};
use nanoid::nanoid;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    database::user::{self, User},
    AppState,
};

pub enum AuthErrors {
    InvalidEmail,
    EmailAlreadyRegistered,
    InvalidData,
    ServerError(String),
}

impl IntoResponse for AuthErrors {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AuthErrors::EmailAlreadyRegistered => (StatusCode::BAD_REQUEST, "Email already registered".to_string()),
            AuthErrors::InvalidEmail => (StatusCode::BAD_REQUEST, "Invalid email".to_string()),
            AuthErrors::InvalidData => (StatusCode::BAD_REQUEST, "Invalid Parameters".to_string()),
            AuthErrors::ServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, body).into_response()
    }
}

pub struct Login {
    email: String,
    password: String,
}

pub async fn login() {}

#[derive(Debug, Deserialize)]
pub struct Register {
    first_name: String,
    last_name: String,
    email: String,
    password: String,
}

pub async fn register(State(state): State<AppState>, Json(req): Json<Register>) -> Result<StatusCode, AuthErrors> {
    let user = state
        .db
        .model::<user::User>()
        .join("account", "account.user_id = user.id")
        .equals("user.email", req.email.clone())
        .first::<user::User>()
        .await;

    match user {
        Ok(_) => {
            return Err(AuthErrors::EmailAlreadyRegistered);
        }
        Err(sqlx::Error::RowNotFound) => {}
        Err(e) => {
            return Err(AuthErrors::ServerError(e.to_string()));
        }
    };

    let mut tx =
        state.db.begin().await.map_err(|_| AuthErrors::ServerError("Failed to create transaction".to_string()))?;

    let user_id = nanoid!();
    tx.model::<user::User>()
        .insert(&user::User {
            id: user_id.clone(),
            first_name: req.first_name,
            last_name: req.last_name,
            email: req.email,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .map_err(|e| {
            println!("Err: {}", e);
            AuthErrors::ServerError(format!("Failed to create user"))
        })?;

    let acc_id = nanoid!();
    let password = hash(req.password.as_bytes(), DEFAULT_COST)
        .map_err(|e| AuthErrors::ServerError(format!("Failed to create account: {}", e.to_string())))?;
    tx.model::<user::Account>()
        .insert(&user::Account {
            id: acc_id,
            user_id: user_id,
            account_type: "credential".to_string(),
            password: password,
            changed_password: Utc::now(),
            created_at: Utc::now(),
        })
        .await
        .map_err(|e| {
            println!("Err: {}", e);
            AuthErrors::ServerError(format!("Failed to create user"))
        })?;

    tx.commit().await.map_err(|e| {
        println!("Err: {}", e);
        AuthErrors::ServerError(format!("Failed to create user"))
    })?;

    Ok(StatusCode::CREATED)
}

pub async fn list_users(State(state): State<AppState>, Query(pagination): Query<Pagination>) -> Json<Vec<User>> {
    let users = pagination.apply(state.db.model::<User>())
        .scan().await.unwrap();
    
    Json(users)
}
