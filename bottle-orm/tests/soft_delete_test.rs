use bottle_orm::{Database, Model, Op};
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
    db.model::<SoftUser>().filter(soft_user_fields::ID, Op::Eq, user_id.to_string()).delete().await?;

    // 5. Verify user is GONE from standard scope
    let users_after_delete: Vec<SoftUser> = db.model::<SoftUser>().scan().await?;
    assert_eq!(users_after_delete.len(), 0);

    // 6. Verify user STILL EXISTS in with_deleted scope
    let all_users: Vec<SoftUser> = db.model::<SoftUser>().with_deleted().scan().await?;
    assert_eq!(all_users.len(), 1);
    assert!(all_users[0].deleted_at.is_some()); // Timestamp should be set

    // 7. Verify hard delete
    db.model::<SoftUser>()
        .filter(soft_user_fields::ID, Op::Eq, user_id.to_string())
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
    let first = db.model::<SoftUser>().filter(soft_user_fields::ID, Op::Eq, u2.id.to_string()).first::<SoftUser>().await;
    assert!(first.is_err(), "Should not find deleted user in default scope");

    // Update -> Should affect only Active
    let affected = db.model::<SoftUser>().update("name", "Updated").await?;
    assert_eq!(affected, 1, "Should only update active user");

    let u2_refetch = db
        .model::<SoftUser>()
        .with_deleted()
        .filter(soft_user_fields::ID, Op::Eq, u2.id.to_string())
        .first::<SoftUser>()
        .await?;
    assert_eq!(u2_refetch.name, "Deleted", "Deleted user should not be updated");

    Ok(())
}

#[tokio::test]
async fn test_hard_delete() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<SoftUser>().run().await?;

    // Insert 3 users
    let u1 = SoftUser { id: Uuid::new_v4(), name: "User1".to_string(), deleted_at: None };
    let u2 = SoftUser { id: Uuid::new_v4(), name: "User2".to_string(), deleted_at: None };
    let u3 = SoftUser { id: Uuid::new_v4(), name: "User3".to_string(), deleted_at: None };

    db.model::<SoftUser>().insert(&u1).await?;
    db.model::<SoftUser>().insert(&u2).await?;
    db.model::<SoftUser>().insert(&u3).await?;

    // Verify 3 users exist
    let count: i64 = db.model::<SoftUser>().count().await?;
    assert_eq!(count, 3);

    // Hard delete User1 (bypasses soft delete, permanent removal)
    let deleted = db
        .model::<SoftUser>()
        .filter(soft_user_fields::ID, Op::Eq, u1.id.to_string())
        .hard_delete()
        .await?;
    assert_eq!(deleted, 1, "Should delete exactly 1 row");

    // Verify User1 is GONE (even with_deleted)
    let all_users: Vec<SoftUser> = db.model::<SoftUser>().with_deleted().scan().await?;
    assert_eq!(all_users.len(), 2);
    assert!(all_users.iter().all(|u| u.id != u1.id), "User1 should be permanently deleted");

    // Hard delete all remaining users
    let deleted_all = db.model::<SoftUser>().hard_delete().await?;
    assert_eq!(deleted_all, 2, "Should delete 2 remaining rows");

    // Verify all users are GONE
    let final_count: i64 = db.model::<SoftUser>().with_deleted().count().await?;
    assert_eq!(final_count, 0, "All users should be permanently deleted");

    Ok(())
}
