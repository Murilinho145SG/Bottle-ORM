use bottle_orm::{Database, Model};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Model, PartialEq)]
struct Product {
    #[orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub price: f64,
}

#[tokio::test]
async fn test_asterisk_expansion_and_quoting() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::builder().is_test(true).try_init();
    
    // 1. Setup Database
    let db = Database::builder()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    // 2. Run Migrations
    db.migrator()
        .register::<Product>()
        .run()
        .await?;

    // 3. Insert Test Data
    db.model::<Product>().insert(&Product {
        id: 1,
        name: "Laptop".to_string(),
        price: 1500.0,
    }).await?;

    // 4. Test select("p.*") with alias
    let results: Vec<Product> = db.model::<Product>()
        .alias("p")
        .select("p.*")
        .scan()
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Laptop");

    // 5. Test manual selects with dots are quoted correctly
    let sql = db.model::<Product>()
        .alias("p")
        .select("p.id, p.name")
        .to_sql();
    
    // SQL should contain quoted identifiers
    assert!(sql.contains("\"p\".\"id\""));
    assert!(sql.contains("\"p\".\"name\""));

    println!("Asterisk expansion and quoting tests passed!");
    Ok(())
}
