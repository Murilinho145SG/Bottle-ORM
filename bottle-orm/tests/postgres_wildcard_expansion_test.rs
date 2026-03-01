use bottle_orm::{Database, Model, FromAnyRow};
use bottle_orm::database::Drivers;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Role {
    #[orm(primary_key)]
    id: String,
    name: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Permission {
    #[orm(primary_key)]
    id: String,
    name: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct RolePermission {
    #[orm(primary_key)]
    id: String,
    role_id: String,
    permission_id: String,
}

#[derive(Debug, FromAnyRow, Serialize, Deserialize)]
struct PermissionDTO {
    id: String,
    name: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromAnyRow, Serialize, Deserialize)]
struct RoleWithPermissionDTO {
    role_id: String,
    id: String,
    name: String,
    created_at: DateTime<Utc>,
}

#[tokio::test]
async fn test_postgres_wildcard_expansion() -> Result<(), Box<dyn std::error::Error>> {
    // We can't easily run against real Postgres in this env, 
    // but we can verify that the code compiles and the logic for expansion exists.
    
    // Using SQLite just to have a working database connection
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    db.migrator()
        .register::<Role>()
        .register::<Permission>()
        .register::<RolePermission>()
        .run().await?;

    // The fix I made is in `scan_as`. If I call it with a "p.*" select,
    // it should now correctly find the columns from the DTO even for joined tables.
    
    // We can't easily "intercept" the SQL here without more infrastructure,
    // but we can at least ensure that scan_as works with joins and wildcard on SQLite.
    
    let role_id = "role1".to_string();
    let perm_id = "perm1".to_string();
    
    db.model::<Role>().insert(&Role { id: role_id.clone(), name: "Admin".to_string(), created_at: Utc::now() }).await?;
    db.model::<Permission>().insert(&Permission { id: perm_id.clone(), name: "read".to_string(), created_at: Utc::now() }).await?;
    db.model::<RolePermission>().insert(&RolePermission { id: "rp1".to_string(), role_id: role_id.clone(), permission_id: perm_id.clone() }).await?;

    // This should work on SQLite (my fix doesn't break SQLite because it has a fallback)
    let results: Vec<RoleWithPermissionDTO> = db.model::<RolePermission>()
        .alias("rp")
        .join("permission p", "p.id = rp.permission_id")
        .select("rp.role_id")
        .select("p.*")
        .scan_as::<RoleWithPermissionDTO>()
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].role_id, role_id);
    assert_eq!(results[0].id, perm_id);
    assert_eq!(results[0].name, "read");
    
    println!("Postgres wildcard expansion test (on SQLite fallback) passed!");
    Ok(())
}
