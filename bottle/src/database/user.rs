use bottle_orm::{FromAnyRow, Model};
use chrono::{DateTime, Utc};

#[derive(Debug, Model, FromAnyRow)]
pub struct User {
    #[orm(primary_key)]
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    #[orm(unique)]
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Model, FromAnyRow)]
pub struct Account {
	#[orm(primary_key)]
	pub id: String,
	#[orm(foreign_key = "User::id")]
	pub user_id: String,
	pub account_type: String,
	pub password: String,
	pub changed_password: DateTime<Utc>,
	pub created_at: DateTime<Utc>
}
