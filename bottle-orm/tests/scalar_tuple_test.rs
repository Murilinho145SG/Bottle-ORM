use bottle_orm::{Database, Model};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct BannedUser {
    #[orm(primary_key)]
    id: Uuid,
    user_id: Uuid,
    reason: String,
    banned_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

#[tokio::test]
async fn test_scalar_tuple_with_datetime() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;
    db.migrator().register::<BannedUser>().run().await?;

    let user_id = Uuid::new_v4();
    let now = Utc::now();
    let ban = BannedUser {
        id: Uuid::new_v4(),
        user_id,
        reason: "Toxic behavior".to_string(),
        banned_at: now,
        revoked_at: None,
    };
    db.model::<BannedUser>().insert(&ban).await?;

    // Test scalar with tuple (String, DateTime<Utc>)
    let (reason, banned_at): (String, DateTime<Utc>) = db.model::<BannedUser>()
        .equals("user_id", user_id.to_string())
        .is_null("revoked_at")
        .select("reason")
        .select("banned_at")
        .scalar()
        .await?;
    
    assert_eq!(reason, "Toxic behavior");
    // SQLite might lose some precision, so we compare timestamps or formatted strings
    assert_eq!(banned_at.timestamp(), now.timestamp());

    Ok(())
}
