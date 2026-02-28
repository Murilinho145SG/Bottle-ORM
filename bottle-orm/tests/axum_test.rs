use bottle_orm::{Database, Model};
use axum::{extract::State, extract::Path, response::Json, routing::delete, Router};
use serde::{Deserialize, Serialize};

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct User {
    #[orm(primary_key)]
    id: String,
    first_name: String,
    last_name: String,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct BannedUsers {
    #[orm(primary_key)]
    id: i32,
    author_id: String,
    user_id: String,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct Session {
    #[orm(primary_key)]
    id: i32,
    user_id: String,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct Account {
    #[orm(primary_key)]
    id: i32,
    user_id: String,
}

#[derive(Clone)]
struct AppState {
    db: Database,
}

#[derive(Serialize)]
enum Responses {
    Message { message: String },
}

// Simulating your handler
async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Responses>, String> {
    let db = state.db;
    let tx = db.begin().await.map_err(|e| e.to_string())?;
    
    tx.model::<BannedUsers>()
        .or_where_raw("author_id", user_id.clone())
        .or_where_raw("user_id", user_id.clone())
        .hard_delete()
        .await
        .map_err(|e| e.to_string())?;
        
    tx.model::<Session>()
        .equals("user_id", user_id.clone())
        .hard_delete()
        .await
        .map_err(|e| e.to_string())?;
        
    tx.model::<Account>()
        .equals("user_id", user_id.clone())
        .hard_delete()
        .await
        .map_err(|e| e.to_string())?;
        
    let (first_name, last_name): (String, String) = tx
        .model::<User>()
        .equals("id", user_id.clone())
        .select("first_name")
        .select("last_name")
        .first()
        .await
        .map_err(|e| e.to_string())?;
        
    tx.model::<User>()
        .equals("id", user_id)
        .hard_delete()
        .await
        .map_err(|e| e.to_string())?;
        
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(Json(Responses::Message {
        message: format!("{} {} foi banido com sucesso.", first_name, last_name),
    }))
}

#[tokio::test]
async fn test_axum_compilation() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let state = AppState { db };
    
    // The error usually happens when Axum tries to route this function
    let _app: Router = Router::new()
        .route("/users/:id", delete(delete_user))
        .with_state(state);
}
