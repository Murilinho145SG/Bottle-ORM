use bottle_orm::{Database, Model, BottleEnum, Op};
use serde::{Deserialize, Serialize};

#[derive(BottleEnum, Debug, Clone, PartialEq, Serialize, Deserialize)]
enum UserRole {
    Admin,
    User,
    Guest,
}

#[derive(Model, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EnumUser {
    #[orm(primary_key)]
    id: i32,
    name: String,
    #[orm(enum)]
    role: UserRole,
    #[orm(enum)]
    optional_role: Option<UserRole>,
}

#[tokio::test]
async fn test_enum_mapping() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator().register::<EnumUser>().run().await?;

    let user1 = EnumUser {
        id: 1,
        name: "Alice".to_string(),
        role: UserRole::Admin,
        optional_role: Some(UserRole::User),
    };

    let user2 = EnumUser {
        id: 2,
        name: "Bob".to_string(),
        role: UserRole::User,
        optional_role: None,
    };

    db.model::<EnumUser>().insert(&user1).await?;
    db.model::<EnumUser>().insert(&user2).await?;

    // Test scan
    let users: Vec<EnumUser> = db.model::<EnumUser>().order("id ASC").scan().await?;
    assert_eq!(users.len(), 2);
    
    assert_eq!(users[0].role, UserRole::Admin);
    assert_eq!(users[0].optional_role, Some(UserRole::User));
    
    assert_eq!(users[1].role, UserRole::User);
    assert_eq!(users[1].optional_role, None);

    // Test filter by enum
    let admins: Vec<EnumUser> = db.model::<EnumUser>()
        .filter("role", Op::Eq, UserRole::Admin.to_string())
        .scan()
        .await?;
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].name, "Alice");

    println!("Enum mapping test passed!");
    Ok(())
}
