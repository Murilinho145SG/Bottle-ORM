use bottle_orm::{Database, Model, Op};
use uuid::Uuid;

#[derive(Debug, Clone, Model, PartialEq)]
struct TestUser {
    #[orm(primary_key)]
    id: Uuid,
    name: String,
    age: i32,
    active: i32, // Use i32 instead of bool for SQLite Any driver compatibility in this test
}

#[tokio::test]
async fn test_complex_where_clauses() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator().register::<TestUser>().run().await?;

    let users = vec![
        TestUser { id: Uuid::new_v4(), name: "Alice".to_string(), age: 25, active: 1 },
        TestUser { id: Uuid::new_v4(), name: "Bob".to_string(), age: 30, active: 0 },
        TestUser { id: Uuid::new_v4(), name: "Charlie".to_string(), age: 35, active: 1 },
        TestUser { id: Uuid::new_v4(), name: "David".to_string(), age: 40, active: 0 },
    ];

    for user in &users {
        db.model::<TestUser>().insert(user).await?;
    }

    // Test OR filter
    let results: Vec<TestUser> = db.model::<TestUser>()
        .filter("name", Op::Eq, "Alice".to_string())
        .or_filter("name", Op::Eq, "Bob".to_string())
        .scan()
        .await?;
    assert_eq!(results.len(), 2);

    // Test or_between
    let results: Vec<TestUser> = db.model::<TestUser>()
        .between("age", 24, 26)
        .or_between("age", 39, 41)
        .scan()
        .await?;
    assert_eq!(results.len(), 2); // Alice (25) and David (40)
    assert!(results.iter().any(|u| u.name == "Alice"));
    assert!(results.iter().any(|u| u.name == "David"));

    // Test or_in_list
    let results: Vec<TestUser> = db.model::<TestUser>()
        .filter("name", Op::Eq, "Alice".to_string())
        .or_in_list("name", vec!["Bob".to_string(), "Charlie".to_string()])
        .scan()
        .await?;
    assert_eq!(results.len(), 3); // Alice, Bob, Charlie

    // Test or_group
    // WHERE name = 'David' OR (active = 1 AND age > 30)
    let results: Vec<TestUser> = db.model::<TestUser>()
        .filter("name", Op::Eq, "David".to_string())
        .or_group(|q| q.filter("active", Op::Eq, 1).filter("age", Op::Gt, 30))
        .scan()
        .await?;
    assert_eq!(results.len(), 2); // David (active 0) and Charlie (active 1, age 35)
    assert!(results.iter().any(|u| u.name == "David"));
    assert!(results.iter().any(|u| u.name == "Charlie"));

    // Test NOT filter
    let results: Vec<TestUser> = db.model::<TestUser>()
        .not_filter("name", Op::Eq, "Alice".to_string())
        .scan()
        .await?;
    assert_eq!(results.len(), 3);
    assert!(!results.iter().any(|u| u.name == "Alice"));

    // Test OR NOT
    let results: Vec<TestUser> = db.model::<TestUser>()
        .filter("name", Op::Eq, "Alice".to_string())
        .or_not_filter("active", Op::Eq, 1)
        .scan()
        .await?;
    assert_eq!(results.len(), 3);
    assert!(results.iter().any(|u| u.name == "Alice"));
    assert!(results.iter().any(|u| u.name == "Bob"));
    assert!(results.iter().any(|u| u.name == "David"));

    // Test where_raw
    let results: Vec<TestUser> = db.model::<TestUser>()
        .where_raw("name = ?", "Alice".to_string())
        .where_raw("age > ?", 20)
        .scan()
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Alice");

    // Test or_where_raw
    let results: Vec<TestUser> = db.model::<TestUser>()
        .filter("active", Op::Eq, 0)
        .or_where_raw("name = ?", "Alice".to_string())
        .scan()
        .await?;
    // Bob (active 0), David (active 0) + Alice
    assert_eq!(results.len(), 3);

    println!("Complex WHERE clauses test passed!");
    Ok(())
}
