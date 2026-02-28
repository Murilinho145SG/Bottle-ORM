use bottle_orm::{Database, Model};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Model, PartialEq, Serialize, Deserialize)]
struct BatchUser {
    #[orm(primary_key)]
    id: Uuid,
    name: String,
    age: Option<i32>,
}

#[tokio::test]
async fn test_batch_insert() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator().register::<BatchUser>().run().await?;

    let users = vec![
        BatchUser { 
            id: Uuid::new_v4(), 
            name: "Alice".to_string(), 
            age: Some(30) 
        },
        BatchUser { 
            id: Uuid::new_v4(), 
            name: "Bob".to_string(), 
            age: None // Test NULL binding
        },
        BatchUser { 
            id: Uuid::new_v4(), 
            name: "Charlie".to_string(), 
            age: Some(25) 
        },
    ];

    // Execute batch insert
    db.model::<BatchUser>().batch_insert(&users).await?;

    // Verify results
    let fetched_users: Vec<BatchUser> = db.model::<BatchUser>()
        .order("name ASC")
        .scan()
        .await?;
    
    assert_eq!(fetched_users.len(), 3);
    
    // Alice
    assert_eq!(fetched_users[0].name, "Alice");
    assert_eq!(fetched_users[0].age, Some(30));
    
    // Bob
    assert_eq!(fetched_users[1].name, "Bob");
    assert_eq!(fetched_users[1].age, None);
    
    // Charlie
    assert_eq!(fetched_users[2].name, "Charlie");
    assert_eq!(fetched_users[2].age, Some(25));

    println!("Batch insert test passed!");
    Ok(())
}

#[tokio::test]
async fn test_batch_insert_empty() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<BatchUser>().run().await?;

    let users: Vec<BatchUser> = vec![];
    
    // Should not error and do nothing
    db.model::<BatchUser>().batch_insert(&users).await?;
    
    let count = db.model::<BatchUser>().count().await?;
    assert_eq!(count, 0);
    
    Ok(())
}
