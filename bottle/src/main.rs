use std::env;

use bottle_orm::{Database, Model};
use dotenvy::dotenv;
use sqlx::types::chrono::{DateTime, Utc};

#[derive(Model)]
struct User {
    #[orm(primary_key)]
    id: i32,
    #[orm(size = 50, unique)]
    username: String,
    age: i32,
}

#[derive(Model)]
struct Account {
    #[orm(primary_key, size = 21)]
    id: String,
    r#type: String,
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();
    let url = env::var("DATABASE_URL").expect("DATABASE_URL is not defined.");
    let db = Database::connect(&url).await?;
    db.migrator().register::<User>().register::<Account>().run().await?;
    Ok(())
}
