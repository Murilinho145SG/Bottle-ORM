use std::env;

use bottle_orm::Database;

use crate::database::user;

pub async fn initialize() -> Result<Database, Box<dyn std::error::Error>> {
	let url = env::var("DATABASE_URL")?;
	let db = Database::connect(&url).await?;
	db.migrator()
		.register::<user::User>()
		.register::<user::Account>()
		.run()
		.await?;

	Ok(db)
}
