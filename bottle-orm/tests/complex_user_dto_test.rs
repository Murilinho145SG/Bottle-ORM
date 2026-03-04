use bottle_orm::{Database, Model, FromAnyRow, BottleEnum};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, BottleEnum)]
pub enum StatusUser {
    Active,
    Pending,
    Banned,
    Unknown,
}

#[derive(Debug, Model, Clone)]
pub struct User {
    #[orm(primary_key)]
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar: Option<String>,
    pub email: String,
    #[orm(enum)]
    pub status: StatusUser,
    pub role: Option<String>,
    pub stripe_customer_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Model, Clone)]
pub struct Account {
    #[orm(primary_key)]
    pub id: String,
    pub user_id: String,
    pub account_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Model, Clone)]
pub struct Session {
    #[orm(primary_key)]
    pub id: String,
    pub user_id: String,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromAnyRow)]
pub struct UsersGet {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar: Option<String>,
    pub email: String,
    pub status: String,
    pub role: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub account_type: String, 
    pub last_seen: Option<DateTime<Utc>>, 
}

#[tokio::test]
async fn test_list_users_complex_query() -> Result<(), Box<dyn std::error::Error>> {
    // Usando a configuração correta para SQLite em memória
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    
    // Migrations
    db.migrator()
        .register::<User>()
        .register::<Account>()
        .register::<Session>()
        .run()
        .await?;

    let now = Utc::now();
    let user_id = "user_1".to_string();

    // Data Setup
    let user = User {
        id: user_id.clone(),
        first_name: "John".to_string(),
        last_name: "Doe".to_string(),
        avatar: Some("avatar.png".to_string()),
        email: "john@example.com".to_string(),
        status: StatusUser::Active,
        role: Some("admin".to_string()),
        stripe_customer_id: "cus_123".to_string(),
        created_at: now,
        updated_at: now,
    };
    db.model::<User>().insert(&user).await?;

    let account = Account {
        id: "acc_1".to_string(),
        user_id: user_id.clone(),
        account_type: "google".to_string(),
        created_at: now,
    };
    db.model::<Account>().insert(&account).await?;

    let session = Session {
        id: "sess_1".to_string(),
        user_id: user_id.clone(),
        last_seen: now,
    };
    db.model::<Session>().insert(&session).await?;

    // Query Builder - Exatamente como no código do usuário
    let query_builder = db.model::<User>()
        .left_join("session", "session.user_id = user.id")
        .left_join("account", "account.user_id = user.id")
        .select("user.*")
        .select("session.last_seen")
        .select("account.account_type");
    
    let sql = query_builder.to_sql();
    println!("Generated SQL: {}", sql);

    // Teste de mapeamento para o DTO UsersGet
    let results: Vec<UsersGet> = query_builder.scan_as::<UsersGet>().await?;

    assert_eq!(results.len(), 1);
    let dto = &results[0];
    
    assert_eq!(dto.id, user_id);
    assert_eq!(dto.first_name, "John");
    assert_eq!(dto.account_type, "google");
    assert!(dto.last_seen.is_some());
    assert_eq!(dto.status, "active");
    
    // Verificação de Segurança (Aspas em identificadores e tabelas)
    assert!(sql.contains("\"user\".*") || sql.contains("\"user\".\"id\""), "Wildcard ou colunas de user devem estar protegidos");
    assert!(sql.contains("\"session\".\"last_seen\""), "Colunas joinadas devem estar protegidas");
    assert!(sql.contains("\"account\".\"account_type\""), "Colunas joinadas devem estar protegidas");

    Ok(())
}
