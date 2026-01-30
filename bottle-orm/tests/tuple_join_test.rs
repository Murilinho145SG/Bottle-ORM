use bottle_orm::{Database, Model};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct User {
    #[orm(primary_key)]
    id: Uuid,
    #[orm(size = 50, unique)]
    username: String,
    email: String,
    age: i32,
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Account {
    #[orm(primary_key)]
    id: Uuid,
    #[orm(foreign_key = "User::id")]
    user_id: Uuid,
    account_type: String,
    balance: f64,
}

#[tokio::test]
async fn test_tuple_join() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Database (SQLite in-memory)
    // Use max_connections(1) to ensure all operations share the same in-memory database
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // 2. Run Migrations
    db.migrator().register::<User>().register::<Account>().run().await?;

    // 3. Insert Test Data
    let user_id = Uuid::new_v4();
    let account_id = Uuid::new_v4();
    let now = Utc::now();

    let user = User {
        id: user_id,
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        age: 30,
        created_at: now,
    };

    let account = Account { id: account_id, user_id: user_id, account_type: "credential".to_string(), balance: 100.50 };

    db.model::<User>().insert(&user).await?;
    db.model::<Account>().insert(&account).await?;

    // 4. Perform Join Query returning a Tuple
    // This uses the new feature where columns are automatically aliased (e.g. user__id, account__id)
    // and mapped to the respective structs in the tuple.
    let (fetched_user, fetched_account): (User, Account) = db
        .model::<User>()
        .join("account", "account.user_id = user.id")
        .equals("account_type", "credential".to_string())
        .equals("user.email", "test@example.com".to_string())
        .first()
        .await?;

    // 5. Assertions
    assert_eq!(fetched_user.id, user.id);
    assert_eq!(fetched_user.username, user.username);
    assert_eq!(fetched_account.id, account.id);
    assert_eq!(fetched_account.user_id, user.id);
    assert_eq!(fetched_account.account_type, "credential");

    // Check that we didn't mix up IDs (since both tables have 'id' column)
    assert_ne!(fetched_user.id, fetched_account.id);

    println!("Tuple join test passed successfully!");

    Ok(())
}
