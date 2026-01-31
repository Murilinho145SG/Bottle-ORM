use bottle_orm::{Database, Model};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Model, PartialEq)]
struct SoftUser {
    #[orm(primary_key)]
    id: Uuid,
    name: String,

    #[orm(soft_delete)]
    deleted_at: Option<DateTime<Utc>>,
}

#[tokio::test]
async fn test_soft_delete_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // 1. Create Table
    db.migrator().register::<SoftUser>().run().await?;

    // 2. Insert User
    let user_id = Uuid::new_v4();
    let user = SoftUser { id: user_id, name: "Bob".to_string(), deleted_at: None };
    db.model::<SoftUser>().insert(&user).await?;

    // 3. Verify user exists in standard scope
    let users: Vec<SoftUser> = db.model::<SoftUser>().scan().await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Bob");

    // 4. Perform Soft Delete
    db.model::<SoftUser>().filter("id", "=", user_id.to_string()).delete().await?;

    // 5. Verify user is GONE from standard scope
    let users_after_delete: Vec<SoftUser> = db.model::<SoftUser>().scan().await?;
    assert_eq!(users_after_delete.len(), 0);

    // 6. Verify user STILL EXISTS in with_deleted scope
    let all_users: Vec<SoftUser> = db.model::<SoftUser>().with_deleted().scan().await?;
    assert_eq!(all_users.len(), 1);
    assert!(all_users[0].deleted_at.is_some()); // Timestamp should be set

    // 7. Verify hard delete
    db.model::<SoftUser>()
        .filter("id", "=", user_id.to_string())
        .with_deleted() // Need this to find it first!
        .hard_delete()
        .await?;

    // 8. Verify user is GONE permanently
    let final_users: Vec<SoftUser> = db.model::<SoftUser>().with_deleted().scan().await?;
    assert_eq!(final_users.len(), 0);

    println!("Soft delete lifecycle test passed!");
    Ok(())
}

#[tokio::test]
async fn test_queries_with_soft_delete() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<SoftUser>().run().await?;

    let u1 = SoftUser { id: Uuid::new_v4(), name: "Active".to_string(), deleted_at: None };
    let u2 = SoftUser {
        id: Uuid::new_v4(),
        name: "Deleted".to_string(),
        deleted_at: Some(Utc::now()), // Manually inserted as deleted
    };

    db.model::<SoftUser>().insert(&u1).await?;
    db.model::<SoftUser>().insert(&u2).await?;

    // Standard scan -> Only Active
    let scan = db.model::<SoftUser>().scan::<SoftUser>().await?;
    assert_eq!(scan.len(), 1);
    assert_eq!(scan[0].name, "Active");

    // Count -> Only Active
    let count: i64 = db.model::<SoftUser>().count().await?;
    assert_eq!(count, 1);

    // First -> Only Active
    let first = db.model::<SoftUser>().filter("id", "=", u2.id.to_string()).first::<SoftUser>().await;
    assert!(first.is_err(), "Should not find deleted user in default scope");

    // Update -> Should affect only Active
    // We try to update name of ALL users (no filter)
    let affected = db.model::<SoftUser>().update("name", "Updated").await?;
    assert_eq!(affected, 1, "Should only update active user");

    let u2_refetch =
        db.model::<SoftUser>().with_deleted().filter("id", "=", u2.id.to_string()).first::<SoftUser>().await?;
    assert_eq!(u2_refetch.name, "Deleted", "Deleted user should not be updated");

    Ok(())
}
