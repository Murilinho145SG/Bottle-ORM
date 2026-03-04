use bottle_orm::{Database, Model};
use serde::{Serialize, Deserialize};

#[derive(Debug, Model, Serialize, Deserialize, Clone, PartialEq)]
pub struct Collection {
    #[orm(primary_key)]
    pub id: i32,
    pub tags: Vec<String>,
    pub scores: Vec<i32>,
    pub metadata: serde_json::Value,
}

#[tokio::test]
async fn test_array_and_json_support() -> Result<(), Box<dyn std::error::Error>> {
    // Arrays are natively supported in Postgres, but in SQLite we'll store as JSON text
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    
    db.migrator()
        .register::<Collection>()
        .run()
        .await?;

    let tags = vec!["rust".to_string(), "orm".to_string(), "bottle".to_string()];
    let scores = vec![10, 20, 30];
    let metadata = serde_json::json!({
        "version": "0.5.5",
        "author": "Murilinho145SG",
        "features": ["security", "arrays", "json"]
    });

    let item = Collection {
        id: 1,
        tags: tags.clone(),
        scores: scores.clone(),
        metadata: metadata.clone(),
    };

    // Test Insert
    db.model::<Collection>().insert(&item).await?;

    // Test Fetch
    let fetched: Collection = db.model::<Collection>().equals("id", 1).first().await?;
    
    assert_eq!(fetched.tags, tags);
    assert_eq!(fetched.scores, scores);
    assert_eq!(fetched.metadata, metadata);

    // Test Update
    let new_tags = vec!["updated".to_string()];
    db.model::<Collection>()
        .equals("id", 1)
        .update("tags", serde_json::to_string(&new_tags)?)
        .await?;

    let updated: Collection = db.model::<Collection>().equals("id", 1).first().await?;
    assert_eq!(updated.tags, new_tags);

    println!("Array and JSON support test passed!");
    Ok(())
}
