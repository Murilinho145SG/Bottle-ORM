use bottle_orm::{Database, Model, FromAnyRow, Pagination};
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

#[derive(Debug, FromAnyRow, Serialize, Deserialize)]
struct UserDTO {
    username: String,
    age: i32,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Profile {
    #[orm(primary_key)]
    id: Uuid,
    #[orm(foreign_key = "User::id")]
    user_id: Uuid,
    bio: String,
    last_login: DateTime<Utc>,
}

#[derive(Debug, FromAnyRow, Serialize, Deserialize)]
struct UserProfileDTO {
    username: String,
    bio: String,
    last_login: DateTime<Utc>,
}

#[tokio::test]
async fn test_scan_as_with_joins() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::builder().is_test(true).try_init();
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator()
        .register::<User>()
        .register::<Profile>()
        .run().await?;

    let user_id = Uuid::new_v4();
    let user = User {
        id: user_id,
        username: "join_user".to_string(),
        email: "join@example.com".to_string(),
        age: 30,
        created_at: Utc::now(),
    };
    db.model::<User>().insert(&user).await?;

    let profile = Profile {
        id: Uuid::new_v4(),
        user_id,
        bio: "Rust Developer".to_string(),
        last_login: Utc::now(),
    };
    db.model::<Profile>().insert(&profile).await?;

    // Test scan_as with INNER JOIN
    let results: Vec<UserProfileDTO> = db.model::<User>()
        .inner_join("profile", "profile.user_id = user.id")
        .select("user.username")
        .select("profile.bio")
        .select("profile.last_login")
        .debug()
        .scan_as::<UserProfileDTO>()
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].username, "join_user");
    assert_eq!(results[0].bio, "Rust Developer");
    
    println!("Join test passed!");
    Ok(())
}

#[tokio::test]
async fn test_scan_as_and_paginate_as() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::builder().is_test(true).try_init();
    // 1. Setup Database (SQLite in-memory)
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // 2. Run Migrations
    db.migrator().register::<User>().run().await?;

    // 3. Insert Test Data
    for i in 1..=15 {
        let user = User {
            id: Uuid::new_v4(),
            username: format!("user{}", i),
            email: format!("user{}@example.com", i),
            age: 20 + i,
            created_at: Utc::now(),
        };
        db.model::<User>().insert(&user).await?;
    }

    // 4. Test scan_as with user.*
    let dtos: Vec<UserDTO> = db.model::<User>()
        .select("user.*")
        .filter("age", bottle_orm::Op::Gt, 30)
        .order("age ASC")
        .debug()
        .scan_as::<UserDTO>()
        .await?;

    assert!(!dtos.is_empty());
    assert!(dtos.len() > 0);
    for dto in &dtos {
        assert!(dto.age > 30);
        // Verify created_at is populated
        assert!(dto.created_at <= Utc::now());
    }
    assert_eq!(dtos[0].username, "user11"); // 20 + 11 = 31

    // 5. Test paginate_as with empty select (should use DTO fields)
    let pagination = Pagination::new(0, 5, 100);
    let paginated = pagination.paginate_as::<User, _, UserDTO>(
        db.model::<User>()
            .order("username ASC")
    ).await?;

    assert_eq!(paginated.total, 15);
    assert_eq!(paginated.data.len(), 5);
    assert_eq!(paginated.total_pages, 3);
    assert_eq!(paginated.page, 0);
    assert_eq!(paginated.limit, 5);

    // Verify mapping in paginated data
    assert_eq!(paginated.data[0].username, "user1");
    
    // Test second page
    let pagination_page1 = Pagination::new(1, 5, 100);
    let paginated_page1 = pagination_page1.paginate_as::<User, _, UserDTO>(
        db.model::<User>()
            .select("username, age, created_at")
            .order("username ASC")
    ).await?;
    
    assert_eq!(paginated_page1.data.len(), 5);
    assert_eq!(paginated_page1.page, 1);
    
    println!("Scan_as and Paginate_as tests passed successfully!");

    Ok(())
}
