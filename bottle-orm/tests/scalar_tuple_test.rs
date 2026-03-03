use bottle_orm::{Database, Model};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Model)]
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
    let banned = BannedUser {
        id: Uuid::new_v4(),
        user_id,
        reason: "Test reason".to_string(),
        banned_at: now,
        revoked_at: None,
    };

    db.model::<BannedUser>().insert(&banned).await?;

    // This simulates the user's failing query:
    // let (reason, banned_at): (String, DateTime<Utc>) = db.model::<BannedUsers>()
    //     .alias("bu")
    //     .equals("bu.user_id", user.id)
    //     .select("bu.reason")
    //     .select("bu.banned_at")
    //     .is_null("bu.revoked_at")
    //     .scalar()
    //     .await?;

    let (reason, banned_at): (String, DateTime<Utc>) = db.model::<BannedUser>()
        .alias("bu")
        .equals("bu.user_id", user_id.to_string())
        .select("bu.reason")
        .select("bu.banned_at")
        .is_null("bu.revoked_at")
        .scalar()
        .await?;

    assert_eq!(reason, "Test reason");
    // SQLite might lose some precision, but the date should be the same
    assert_eq!(banned_at.timestamp(), now.timestamp());

    println!("Scalar tuple with datetime test passed!");
    Ok(())
}
