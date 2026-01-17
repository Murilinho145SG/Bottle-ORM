# Bottle ORM

Bottle ORM is a simple, asynchronous Object-Relational Mapper (ORM) for Rust, built on top of [SQLx](https://github.com/launchbadge/sqlx). It provides a lightweight and easy-to-use interface for interacting with databases, leveraging Rust's type system and async capabilities.

## Features

- **Async First**: Built from the ground up for asynchronous Rust using `tokio` and `sqlx`.
- **Derive Macros**: Define your schema using simple Rust structs with `#[derive(Model)]`.
- **Auto-Migration**: Automatically create tables based on your struct definitions.
- **Type Safety**: Leverages Rust's strong type system to map SQL types to Rust types.
- **SQLx Powered**: Uses the robust SQLx library for database interactions.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
bottle-orm = { git = "https://github.com/your-username/bottle-orm" }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["runtime-tokio-native-tls", "postgres"] } # Or your preferred db
dotenvy = "0.15"
```

## Usage

Here's a quick example of how to define models and connect to the database.

### 1. Define your Models

```rust
use bottle_orm::Model;
use sqlx::types::chrono::{DateTime, Utc};

#[derive(Model)]
struct User {
    #[orm(primary_key)]
    id: i32,
    #[orm(size = 50, unique)]
    username: String,
    age: i32,
}

#[derive(Model)]
struct Account {
    #[orm(primary_key, size = 21)]
    id: String,
    r#type: String, // Escape reserved keywords with r#
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}
```

### 2. Connect and Migrate

```rust
use std::env;
use bottle_orm::Database;
use dotenvy::dotenv;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();
    let url = env::var("DATABASE_URL").expect("DATABASE_URL is not defined.");
    
    // Connect to the database
    let db = Database::connect(&url).await?;
    
    // Run migrations
    db.migrator()
        .register::<User>()
        .register::<Account>()
        .run()
        .await?;
        
    println!("Migrations completed successfully!");
    
    Ok(())
}
```

## Supported Types

Bottle ORM currently supports mapping the following Rust types to SQL types:

- `i32` -> `INTEGER`
- `i64` -> `BIGINT`
- `String` -> `TEXT` (or `VARCHAR(n)` if `size = n` is specified)
- `bool` -> `BOOLEAN`
- `f64` -> `DOUBLE PRECISION`
- `Option<T>` -> Nullable column
- `DateTime<Utc>` -> `TIMESTAMPTZ`
- `NaiveDateTime` -> `TIMESTAMP`
- `NaiveDate` -> `DATE`
- `NaiveTime` -> `TIME`

## Contributing

Contributions are welcome! Please look at the [CONTRIBUTING.md](CONTRIBUTING.md) file for details.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
