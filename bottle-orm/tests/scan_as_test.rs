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

    // 4. Test scan_as
    let dtos: Vec<UserDTO> = db.model::<User>()
        .select("username, age, created_at")
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

    // 5. Test paginate_as
    let pagination = Pagination::new(0, 5, 100);
    let paginated = pagination.paginate_as::<User, _, UserDTO>(
        db.model::<User>()
            .select("username, age, created_at")
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
    // user1, user10, user11, user12, user13 are first 5 in Lexicographical order? 
    // user1, user10, user11, user12, user13, user14, user15, user2, ...
    
    println!("Scan_as and Paginate_as tests passed successfully!");

    Ok(())
}
