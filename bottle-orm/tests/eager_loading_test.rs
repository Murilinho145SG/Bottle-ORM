use bottle_orm::{Database, Model, Op};
use serde::{Deserialize, Serialize};

#[derive(Model, Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserAccount {
    #[orm(primary_key)]
    id: i32,
    username: String,

    #[orm(has_many = "UserPost", foreign_key = "user_id")]
    posts: Vec<UserPost>,

    #[orm(has_one = "UserProfile", foreign_key = "user_id")]
    profile: Option<UserProfile>,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserPost {
    #[orm(primary_key)]
    id: i32,
    user_id: i32,
    title: String,

    #[orm(belongs_to = "UserAccount", foreign_key = "user_id")]
    user: Option<UserAccount>,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserProfile {
    #[orm(primary_key)]
    id: i32,
    user_id: i32,
    bio: String,
}

#[tokio::test]
async fn test_eager_loading_comprehensive() -> Result<(), Box<dyn std::error::Error>> {
    // max_connections(1) is critical for sqlite::memory: to work with pools
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    db.create_table::<UserAccount>().await?;
    db.create_table::<UserPost>().await?;
    db.create_table::<UserProfile>().await?;

    // Seed data
    let user1 = UserAccount { id: 1, username: "user1".to_string(), posts: vec![], profile: None };
    let user2 = UserAccount { id: 2, username: "user2".to_string(), posts: vec![], profile: None };

    db.model::<UserAccount>().insert(&user1).await?;
    db.model::<UserAccount>().insert(&user2).await?;

    let post1 = UserPost { id: 1, user_id: 1, title: "post1".to_string(), user: None };
    let post2 = UserPost { id: 2, user_id: 1, title: "post2".to_string(), user: None };
    let post3 = UserPost { id: 3, user_id: 2, title: "post3".to_string(), user: None };

    db.model::<UserPost>().insert(&post1).await?;
    db.model::<UserPost>().insert(&post2).await?;
    db.model::<UserPost>().insert(&post3).await?;

    let profile1 = UserProfile { id: 1, user_id: 1, bio: "bio1".to_string() };
    db.model::<UserProfile>().insert(&profile1).await?;

    // 1. Test eager loading has_many
    let users = db.model::<UserAccount>()
        .with("posts")
        .order("id ASC")
        .scan_with()
        .await?;

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].username, "user1");
    assert_eq!(users[0].posts.len(), 2);
    assert_eq!(users[0].posts[0].title, "post1");
    assert_eq!(users[0].posts[1].title, "post2");

    assert_eq!(users[1].username, "user2");
    assert_eq!(users[1].posts.len(), 1);
    assert_eq!(users[1].posts[0].title, "post3");

    // 2. Test eager loading has_one
    let users_with_profile = db.model::<UserAccount>()
        .with("profile")
        .filter("id", Op::Eq, 1)
        .scan_with()
        .await?;

    assert_eq!(users_with_profile.len(), 1);
    assert!(users_with_profile[0].profile.is_some());
    assert_eq!(users_with_profile[0].profile.as_ref().unwrap().bio, "bio1");

    // 3. Test eager loading belongs_to
    let posts = db.model::<UserPost>()
        .with("user")
        .filter("id", Op::Eq, 1)
        .scan_with()
        .await?;

    assert_eq!(posts.len(), 1);
    assert!(posts[0].user.is_some());
    assert_eq!(posts[0].user.as_ref().unwrap().username, "user1");

    // 4. Test eager loading multiple relations
    let users_all = db.model::<UserAccount>()
        .with("posts")
        .with("profile")
        .filter("id", Op::Eq, 1)
        .scan_with()
        .await?;

    assert_eq!(users_all.len(), 1);
    assert_eq!(users_all[0].posts.len(), 2);
    assert!(users_all[0].profile.is_some());

    Ok(())
}
