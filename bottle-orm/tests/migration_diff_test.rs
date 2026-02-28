use bottle_orm::{Database, Model, ColumnInfo};
use uuid::Uuid;
use std::collections::HashMap;

// Versão 1 do Model
#[derive(Debug, Clone, PartialEq)]
struct UserV1 {
    id: Uuid,
    name: String,
}

impl Model for UserV1 {
    fn table_name() -> &'static str { "users_evolution" }
    fn columns() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo { name: "id", sql_type: "UUID", is_primary_key: true, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
            ColumnInfo { name: "name", sql_type: "TEXT", is_primary_key: false, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
        ]
    }
    fn active_columns() -> Vec<&'static str> { vec!["id", "name"] }
    fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("id".to_string(), self.id.to_string());
        map.insert("name".to_string(), self.name.clone());
        map
    }
}

// Versão 2 do Model (Adiciona 'age' e um índice em 'email')
#[derive(Debug, Clone, PartialEq)]
struct UserV2 {
    id: Uuid,
    name: String,
    age: i32,
    email: String,
}

impl Model for UserV2 {
    fn table_name() -> &'static str { "users_evolution" }
    fn columns() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo { name: "id", sql_type: "UUID", is_primary_key: true, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
            ColumnInfo { name: "name", sql_type: "TEXT", is_primary_key: false, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
            ColumnInfo { name: "age", sql_type: "INTEGER", is_primary_key: false, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
            ColumnInfo { name: "email", sql_type: "TEXT", is_primary_key: false, is_nullable: false, create_time: false, update_time: false, unique: false, index: true, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
        ]
    }
    fn active_columns() -> Vec<&'static str> { vec!["id", "name", "age", "email"] }
    fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("id".to_string(), self.id.to_string());
        map.insert("name".to_string(), self.name.clone());
        map.insert("age".to_string(), self.age.to_string());
        map.insert("email".to_string(), self.email.clone());
        map
    }
}

#[tokio::test]
async fn test_migration_diffing_evolution() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // 1. Criar V1
    db.sync_table::<UserV1>().await?;
    
    // Inserir dado na V1
    let id = Uuid::new_v4();
    db.raw("INSERT INTO users_evolution (id, name) VALUES (?, ?)")
        .bind(id.to_string())
        .bind("Alice".to_string())
        .execute().await?;

    // 2. Rodar diffing para V2
    db.sync_table::<UserV2>().await?;

    // 3. Verificar se as colunas novas existem
    let columns = db.get_table_columns("users_evolution").await?;
    assert!(columns.contains(&"age".to_string()));
    assert!(columns.contains(&"email".to_string()));

    // 4. Tentar inserir e ler com o novo Model
    let id2 = Uuid::new_v4();
    db.raw("INSERT INTO users_evolution (id, name, age, email) VALUES (?, ?, ?, ?)")
        .bind(id2.to_string())
        .bind("Bob".to_string())
        .bind(30)
        .bind("bob@example.com".to_string())
        .execute().await?;

    println!("Migration Diffing test passed!");
    Ok(())
}

#[tokio::test]
async fn test_migration_index_diffing() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::builder().max_connections(1).connect("sqlite::memory:").await?;

    // 1. Criar V1 (sem índice em name)
    db.sync_table::<UserV1>().await?;
    
    let indexes = db.get_table_indexes("users_evolution").await?;
    // SQLite might have internal indexes for PK, but shouldn't have idx_users_evolution_name
    assert!(!indexes.contains(&"idx_users_evolution_name".to_string()));

    // 2. Definir Model V1.5 (com índice em name)
    #[derive(Debug, Clone, PartialEq)]
    struct UserV1_5 {
        id: Uuid,
        name: String,
    }

    impl Model for UserV1_5 {
        fn table_name() -> &'static str { "users_evolution" }
        fn columns() -> Vec<ColumnInfo> {
            vec![
                ColumnInfo { name: "id", sql_type: "UUID", is_primary_key: true, is_nullable: false, create_time: false, update_time: false, unique: false, index: false, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
                ColumnInfo { name: "name", sql_type: "TEXT", is_primary_key: false, is_nullable: false, create_time: false, update_time: false, unique: false, index: true, foreign_table: None, foreign_key: None, omit: false, soft_delete: false },
            ]
        }
        fn active_columns() -> Vec<&'static str> { vec!["id", "name"] }
        fn to_map(&self) -> HashMap<String, String> {
            let mut map = HashMap::new();
            map.insert("id".to_string(), self.id.to_string());
            map.insert("name".to_string(), self.name.clone());
            map
        }
    }

    // 3. Rodar sync
    db.sync_table::<UserV1_5>().await?;

    // 4. Verificar se o índice foi criado
    let indexes = db.get_table_indexes("users_evolution").await?;
    assert!(indexes.contains(&"idx_users_evolution_name".to_string()));

    println!("Migration Index Diffing test passed!");
    Ok(())
}
