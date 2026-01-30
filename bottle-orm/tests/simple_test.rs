use bottle_orm::{Database, Model};
use uuid::Uuid;

#[derive(Debug, Clone, Model, PartialEq)]
struct SimpleUser {
    #[orm(primary_key)]
    id: Uuid,
    name: String,
}

#[tokio::test]
async fn test_simple_mapping_still_works() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator().register::<SimpleUser>().run().await?;

    let user = SimpleUser { id: Uuid::new_v4(), name: "Alice".to_string() };

    db.model::<SimpleUser>().insert(&user).await?;

    // Test scan simple
    let users: Vec<SimpleUser> = db.model::<SimpleUser>().scan().await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[0].id, user.id);

    // Test first simple
    let fetched: SimpleUser = db.model::<SimpleUser>().first().await?;
    assert_eq!(fetched.name, "Alice");

    println!("Simple mapping test passed!");
    Ok(())
}
