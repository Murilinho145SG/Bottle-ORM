use bottle_orm::Database;

mod database;
mod handlers;
mod server;

#[derive(Clone)]
pub struct AppState {
	db: Database
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	dotenvy::dotenv().ok();
	let db = database::initialize().await?;
	
	server::start_http(AppState { db }).await?;
	Ok(())
}
