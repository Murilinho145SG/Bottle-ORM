use bottle_orm::{Database, Model};
use uuid::Uuid;

#[derive(Debug, Clone, Model, PartialEq)]
struct RolePermission {
    #[orm(primary_key)]
    role_id: Uuid,
    #[orm(primary_key)]
    permission_id: Uuid,
}

#[tokio::test]
async fn test_composite_primary_key_migration() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // This should fail if the bug is present
    let result = db.migrator().register::<RolePermission>().run().await;
    
    match result {
        Ok(_) => {
            println!("Migration succeeded!");
            Ok(())
        },
        Err(e) => {
            println!("Migration failed as expected: {:?}", e);
            Err(e.into())
        }
    }
}
