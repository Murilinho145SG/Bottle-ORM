use bottle_orm::{Database, Model, FromAnyRow, Pagination};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct User {
    #[orm(primary_key)]
    pub id: Uuid,
    #[orm(size = 50, unique)]
    pub username: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Session {
    #[orm(primary_key)]
    pub id: Uuid,
    #[orm(foreign_key = "User::id")]
    pub user_id: Uuid,
    pub last_seen: DateTime<Utc>,
    pub token_id: Option<Uuid>,
}

#[derive(Debug, FromAnyRow, Serialize, Deserialize)]
struct UserDetailDTO {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub last_seen: Option<DateTime<Utc>>,
    pub token_id: Option<Uuid>,
}

#[derive(Debug, FromAnyRow, Serialize, Deserialize, PartialEq)]
struct UserSimpleDTO {
    pub id: Uuid,
    pub username: String,
    pub last_seen: Option<DateTime<Utc>>,
    pub session_uuid: Option<Uuid>,
}

#[tokio::test]
async fn test_pagination_with_left_join_and_option_types() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::builder().is_test(true).try_init();
    
    // 1. Setup Database
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    // 2. Run Migrations
    db.migrator()
        .register::<User>()
        .register::<Session>()
        .run()
        .await?;

    // 3. Insert Test Data
    // User 1: Has a session
    let user1_id = Uuid::new_v4();
    let user1 = User {
        id: user1_id,
        username: "user_with_session".to_string(),
        email: "user1@example.com".to_string(),
    };
    db.model::<User>().insert(&user1).await?;

    let session1_id = Uuid::new_v4();
    let token1_id = Uuid::new_v4();
    let last_seen_val = Utc::now();
    let session1 = Session {
        id: session1_id,
        user_id: user1_id,
        last_seen: last_seen_val,
        token_id: Some(token1_id),
    };
    db.model::<Session>().insert(&session1).await?;

    // User 2: Has no session
    let user2_id = Uuid::new_v4();
    let user2 = User {
        id: user2_id,
        username: "user_without_session".to_string(),
        email: "user2@example.com".to_string(),
    };
    db.model::<User>().insert(&user2).await?;

    // User 3: Another user without session
    let user3_id = Uuid::new_v4();
    let user3 = User {
        id: user3_id,
        username: "another_user".to_string(),
        email: "user3@example.com".to_string(),
    };
    db.model::<User>().insert(&user3).await?;

    // 4. Test Pagination with LEFT JOIN
    let pagination = Pagination::new(0, 10);
    
    // We want to see all 3 users, even if they don't have sessions
    let result = pagination.paginate_as::<User, _, UserSimpleDTO>(
        db.model::<User>()
            .left_join("session", "session.user_id = user.id")
            .select("user.id")
            .select("user.username")
            .select("session.last_seen")
            .select("session.token_id as session_uuid")
            .order("user.username ASC")
            .debug()
    ).await?;

    // Assertions
    assert_eq!(result.total, 3, "Should count all 3 users with LEFT JOIN");
    assert_eq!(result.data.len(), 3);
    
    // Check user without session (another_user)
    let user_no_session = result.data.iter().find(|u| u.username == "another_user").unwrap();
    assert_eq!(user_no_session.id, user3_id);
    assert!(user_no_session.last_seen.is_none(), "last_seen should be None for user without session");
    assert!(user_no_session.session_uuid.is_none(), "session_uuid should be None for user without session");

    // Check user with session
    let user_with_session = result.data.iter().find(|u| u.username == "user_with_session").unwrap();
    assert_eq!(user_with_session.id, user1_id);
    assert!(user_with_session.last_seen.is_some(), "last_seen should be Some for user with session");
    assert_eq!(user_with_session.session_uuid, Some(token1_id), "session_uuid should match token1_id");

    println!("Pagination with LEFT JOIN and Option types test passed!");
    Ok(())
}
