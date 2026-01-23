use axum::{Router, routing::{get, post}};

use crate::{AppState, handlers::auth};

pub async fn start_http(a_state: AppState) -> Result<(), Box<dyn std::error::Error>> {
	let auth_group = Router::new()
		.route("/login", post(auth::login))
		.route("/register", post(auth::register))
		.route("/list", get(auth::list_users));

	let app = Router::new()
		.nest("/auth", auth_group)
		.with_state(a_state);

	let listener = tokio::net::TcpListener::bind("0.0.0.0:7800").await?;
	axum::serve(listener, app).await?;
	Ok(())
}
