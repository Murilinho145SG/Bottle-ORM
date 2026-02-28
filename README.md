# Bottle ORM

[![Crates.io](https://img.shields.io/crates/v/bottle-orm.svg)](https://crates.io/crates/bottle-orm)
[![Docs.rs](https://docs.rs/bottle-orm/badge.svg)](https://docs.rs/bottle-orm)
[![License](https://img.shields.io/crates/l/bottle-orm.svg)](https://github.com/Murilinho145SG/bottle-orm/blob/main/LICENSE)

**Bottle ORM** is a lightweight, async ORM for Rust built on top of [sqlx](https://github.com/launchbadge/sqlx). It is designed to be simple, efficient, and easy to use, providing a fluent Query Builder, automatic schema migrations, and high-performance batch operations.

## Key Features

- **Async & Non-blocking**: Built on `tokio` and `sqlx`.
- **Multi-Driver Support**: PostgreSQL, MySQL, and SQLite (via `sqlx::Any`).
- **Macro-based Models**: Define your schema using standard Rust structs with `#[derive(Model)]`.
- **Fluent Query Builder**: Chainable methods for filtering, selecting, pagination, and sorting.
- **Batch Operations**: High-performance `batch_insert` for multiple records.
- **Native Enum Support**: Easy mapping of Rust Enums to database columns.
- **Auto-Migration & Diffing**: Automatically creates tables and **synchronizes schema changes** (ALTER TABLE) based on your structs.
- **UUID Support**: Full support for UUID versions 1 through 7.

## Installation

Add `bottle-orm` to your `Cargo.toml`.

```toml
[dependencies]
bottle-orm = "0.4.16"
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-native-tls", "any", "postgres", "sqlite", "mysql", "chrono", "uuid"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.11", features = ["serde", "v7"] }
```

## Quick Start

### 1. Define your Models

Use `#[derive(Model)]` for tables and `#[derive(BottleEnum)]` for type-safe enums.

```rust
use bottle_orm::{Model, BottleEnum};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(BottleEnum, Debug, Clone, Serialize, Deserialize, PartialEq)]
enum UserRole {
    Admin,
    User,
    Guest,
}

#[derive(Model, Debug, Clone, Serialize, Deserialize)]
struct User {
    #[orm(primary_key)]
    id: i32,
    #[orm(size = 50, unique, index)]
    username: String,
    #[orm(enum)]
    role: UserRole,
    age: Option<i32>,
    #[orm(create_time)]
    created_at: DateTime<Utc>,
}
```

### 2. Connect and Migrate (with Auto-Diff)

Bottle ORM not only creates tables but also detects missing columns or indexes and adds them automatically.

```rust
use bottle_orm::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::connect("sqlite::memory:").await?;

    // Register models and run migrations
    db.migrator()
        .register::<User>()
        .run()
        .await?;
    
    Ok(())
}
```

### 3. High-Performance Batch Insert

Insert hundreds of records in a single SQL statement.

```rust
let users = vec![
    User { username: "alice".to_string(), role: UserRole::Admin, ... },
    User { username: "bob".to_string(), role: UserRole::User, ... },
];

db.model::<User>().batch_insert(&users).await?;
```

### 4. Advanced Query Builder

Filter data using complex logic, nested groups, and raw SQL fragments.

```rust
use bottle_orm::Op;

let users: Vec<User> = db.model::<User>()
    .filter("age", Op::Gte, 18)
    .and_group(|q| {
        q.filter("role", Op::Eq, "admin")
         .or_filter("role", Op::Eq, "user")
    })
    .where_raw("LENGTH(username) > ?", vec![5])
    .order("created_at DESC")
    .limit(10)
    .scan()
    .await?;
```

## Supported Attributes (`#[orm(...)]`)

- `primary_key`: Marks the column as the Primary Key.
- `unique`: Adds a UNIQUE constraint.
- `index`: Creates an index for this column.
- `enum`: Marks the field as a native Enum (handled via `Display`/`FromStr`).
- `create_time`: Sets default value to `CURRENT_TIMESTAMP`.
- `update_time`: Auto-updates timestamp on modification.
- `foreign_key = "Table::Column"`: Creates a Foreign Key relationship.
- `size = N`: Sets the column size (e.g., `VARCHAR(N)`).
- `omit`: Excludes the column from `SELECT *` by default (useful for passwords).
- `soft_delete`: Enables soft delete functionality.

## Soft Delete

Mark a column with `#[orm(soft_delete)]` to enable automatic filtering.

```rust
#[derive(Model)]
struct Product {
    #[orm(primary_key)]
    id: i32,
    name: String,
    #[orm(soft_delete)]
    deleted_at: Option<DateTime<Utc>>,
}

// Automatically excludes deleted records
let active = db.model::<Product>().scan().await?;

// Include them if needed
let all = db.model::<Product>().with_deleted().scan().await?;
```

## Joins and DTOs

Map complex results to custom structs (DTOs).

```rust
#[derive(FromAnyRow)]
struct UserStats {
    username: String,
    post_count: i64,
}

let stats: Vec<UserStats> = db.model::<User>()
    .alias("u")
    .left_join("Post p ON p.user_id = u.id")
    .select("u.username, COUNT(p.id) as post_count")
    .group_by("u.username")
    .scan_as()
    .await?;
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
