use bottle_orm::{Database, Model};
use serde::{Deserialize, Serialize};

#[derive(Model, Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Permission {
    #[orm(primary_key)]
    id: i32,
    name: String,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize, PartialEq)]
struct RolePermission {
    #[orm(primary_key)]
    id: i32,
    role_id: i32,
    permission_id: i32,
}

#[tokio::test]
async fn test_join_raw_with_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    // Using a single connection to ensure memory DB persistence
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:?cache=shared")
        .await?;

    db.migrator()
        .register::<Permission>()
        .register::<RolePermission>()
        .run()
        .await?;

    // Insert test data
    let p1 = Permission { id: 1, name: "read".to_string() };
    db.model::<Permission>().insert(&p1).await?;

    let rp1 = RolePermission { id: 1, role_id: 10, permission_id: 1 };
    db.model::<RolePermission>().insert(&rp1).await?;

    let role_id = 10;
    
    // Test join_raw with placeholder
    // We use the new join_raw method that supports parameter binding
    let permissions: Vec<Permission> = db.model::<Permission>()
        .join_raw("role_permission rp", "rp.permission_id = permission.id AND rp.role_id = ?", role_id)
        .scan()
        .await?;

    assert_eq!(permissions.len(), 1);
    assert_eq!(permissions[0].name, "read");

    println!("Join raw test passed!");
    Ok(())
}
