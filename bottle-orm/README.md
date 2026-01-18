# Bottle ORM

[![Crates.io](https://img.shields.io/crates/v/bottle-orm.svg)](https://crates.io/crates/bottle-orm)
[![Docs.rs](https://docs.rs/bottle-orm/badge.svg)](https://docs.rs/bottle-orm)
[![License](https://img.shields.io/crates/l/bottle-orm.svg)](https://github.com/Murilinho145SG/bottle-orm/blob/main/LICENSE)

**Bottle ORM** is a lightweight, async ORM for Rust built on top of [sqlx](https://github.com/launchbadge/sqlx). It is designed to be simple, efficient, and easy to use, providing a fluent Query Builder and automatic schema migrations.

## Features

- **Async & Non-blocking**: Built on `tokio` and `sqlx`.
- **Multi-Driver Support**: Compatible with PostgreSQL, MySQL, and SQLite (via `sqlx::Any`).
- **Macro-based Models**: Define your schema using standard Rust structs with `#[derive(Model)]`.
- **Fluent Query Builder**: Chainable methods for filtering, selecting, pagination, and sorting.
- **Auto-Migration**: Automatically creates tables and foreign key constraints based on your structs.

## Installation

Add `bottle-orm` to your `Cargo.toml`. You will also need `sqlx`, `tokio`, and `serde`.

```toml
[dependencies]
bottle-orm = "0.1.0"
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-native-tls", "any", "postgres", "sqlite", "mysql", "chrono"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
```

## Quick Start

### 1. Define your Models

Use the `#[derive(Model)]` macro to define your database tables. You can use the `#[orm(...)]` attribute to configure columns.

```rust
use bottle_orm::Model;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
struct User {
    #[orm(primary_key)]
    id: i32,
    #[orm(size = 50, unique)]
    username: String,
    age: i32,
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
struct Post {
    #[orm(primary_key)]
    id: i32,
    #[orm(foreign_key = "User::id")]
    user_id: i32,
    title: String,
    content: String,
}
```

### 2. Connect and Migrate

Initialize the database connection and run migrations to create tables automatically.

```rust
use bottle_orm::Database;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables (DATABASE_URL)
    // dotenvy::dotenv().ok();
    // let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let database_url = "sqlite::memory:"; // Example for SQLite

    // 1. Connect to the database
    let db = Database::connect(&database_url).await?;

    // 2. Run Migrations (Create tables and FKs)
    db.migrator()
        .register::<User>()
        .register::<Post>()
        .run()
        .await?;

    println!("Database migration completed!");
    
    Ok(())
}
```

### 3. Insert Data

```rust
let new_user = User {
    id: 1,
    username: "alice".to_string(),
    age: 30,
    created_at: chrono::Utc::now(),
};

db.model::<User>().insert(&new_user).await?;
```

### 4. Query Data

Use the fluent query builder to filter and retrieve data.

```rust
// Fetch a single user by ID
let user: User = db.model::<User>()
    .filter("id", "=", 1)
    .first()
    .await?;

println!("Found user: {:?}", user);

// Fetch multiple records with conditions, order, and pagination
let adults: Vec<User> = db.model::<User>()
    .filter("age", ">=", 18)
    .order("age DESC")
    .limit(10)
    .scan()
    .await?;

for u in adults {
    println!("Adult user: {}", u.username);
}
```

## Supported Attributes (`#[orm(...)]`)

- `primary_key`: Marks the column as the Primary Key.
- `unique`: Adds a UNIQUE constraint.
- `create_time`: Sets default value to current timestamp on creation.
- `foreign_key = "Table::Column"`: Creates a Foreign Key relationship.
- `size = N`: (Optional) Hints the size for text columns (mostly for docs/optimization).
- `index`: Creates an index for this column.

## Database Support

Bottle ORM uses `sqlx::Any` to support multiple databases. The driver is detected automatically from the connection string scheme:

- `postgres://...` -> PostgreSQL
- `mysql://...` -> MySQL
- `sqlite://...` -> SQLite

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.