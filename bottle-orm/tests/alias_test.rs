use bottle_orm::{Database, Model, Op};
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
struct Profile {
    #[orm(primary_key)]
    id: Uuid,
    #[orm(foreign_key = "User::id")]
    user_id: Uuid,
    bio: String,
}

#[tokio::test]
async fn test_alias_basic() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<User>().run().await?;

    let user_id = Uuid::new_v4();
    let user = User {
        id: user_id,
        username: "aliased_user".to_string(),
        email: "alias@example.com".to_string(),
        age: 25,
        created_at: Utc::now(),
    };
    db.model::<User>().insert(&user).await?;

    // Test simple alias with filter
    let fetched: User = db.model::<User>()
        .alias("u")
        .filter("username", Op::Eq, "aliased_user".to_string())
        .first()
        .await?;
    
    assert_eq!(fetched.id, user_id);

    // Test alias with explicit prefix in filter
    let fetched2: User = db.model::<User>()
        .alias("u")
        .filter("u.username", Op::Eq, "aliased_user".to_string())
        .first()
        .await?;
    
    assert_eq!(fetched2.id, user_id);

    Ok(())
}

#[tokio::test]
async fn test_alias_join() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<User>().register::<Profile>().run().await?;

    let user_id = Uuid::new_v4();
    let user = User {
        id: user_id,
        username: "join_user".to_string(),
        email: "join@example.com".to_string(),
        age: 30,
        created_at: Utc::now(),
    };
    db.model::<User>().insert(&user).await?;

    let profile_id = Uuid::new_v4();
    let profile = Profile {
        id: profile_id,
        user_id,
        bio: "I love aliases!".to_string(),
    };
    db.model::<Profile>().insert(&profile).await?;

    // Test join with aliases
    let results: Vec<(User, Profile)> = db.model::<User>()
        .alias("u")
        .join("profile p", "p.user_id = u.id")
        .filter("p.bio", Op::Like, "%aliases%".to_string())
        .scan()
        .await?;

    assert_eq!(results.len(), 1);
    let (u, p) = &results[0];
    assert_eq!(u.id, user_id);
    assert_eq!(p.id, profile_id);

    Ok(())
}

#[tokio::test]
async fn test_alias_scan_as() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<User>().run().await?;

    let user_id = Uuid::new_v4();
    let user = User {
        id: user_id,
        username: "scan_as_user".to_string(),
        email: "scan@example.com".to_string(),
        age: 20,
        created_at: Utc::now(),
    };
    db.model::<User>().insert(&user).await?;

    #[derive(Debug, bottle_orm::FromAnyRow)]
    struct UserDTO {
        username: String,
    }

    // Test scan_as with alias
    let dtos: Vec<UserDTO> = db.model::<User>()
        .alias("u")
        .select("u.username")
        .scan_as::<UserDTO>()
        .await?;

    assert_eq!(dtos.len(), 1);
    assert_eq!(dtos[0].username, "scan_as_user");

    // Test scan_as with automatic columns and alias
    let dtos2: Vec<UserDTO> = db.model::<User>()
        .alias("u")
        .scan_as::<UserDTO>()
        .await?;

    assert_eq!(dtos2.len(), 1);
    assert_eq!(dtos2[0].username, "scan_as_user");

    Ok(())
}
