use bottle_orm::Model;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct User {
    #[orm(primary_key)]
    id: String,
    #[orm(size = 50)]
    username: String,
    age: i32,
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct Post {
    #[orm(primary_key)]
    id: String,
    #[orm(foreign_key = "User::id")]
    user_id: String,
    title: String,
    content: String,
    created_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let _ = fs::remove_file("test.db");
    // File::create("test.db")?;
    let database_url = "sqlite://test.db";

    // Connect to the database
    let db = bottle_orm::Database::connect(&database_url).await?;

    // Run Migrations
    db.migrator().register::<User>().register::<Post>().run().await?;

    println!("Database migration completed!");

    // Insert a user
    let new_user_id = Uuid::new_v4().to_string();
    let new_user =
        User { id: new_user_id.clone(), username: "alice".to_string(), age: 30, created_at: chrono::Utc::now() };

    println!("User Model: {:?}", new_user);
    db.model::<User>().insert(&new_user).await?;
    println!("Inserted user: {}, {}", new_user.username, new_user.id);

    // Insert a post
    let new_post = Post {
        id: Uuid::new_v4().to_string(),
        user_id: new_user_id.clone(),
        title: "Hello World".to_string(),
        content: "This is a test post.".to_string(),
        created_at: chrono::Utc::now(),
    };

    db.model::<Post>().insert(&new_post).await?;
    println!("Inserted post: {}", new_post.title);

    // Query user
    let user: User =
        db.model::<User>().omit(user_fields::AGE).filter(user_fields::ID, "=", new_user_id.clone()).first().await?;

    println!("Found user: {:?}", user);

    // Query posts
    let posts: Vec<Post> =
        db.model::<Post>().omit(post_fields::CONTENT).equals(post_fields::USER_ID, new_user_id.clone()).scan().await?;

    for post in posts {
        println!("Post: {:?}", post.content);
    }

    Ok(())
}
