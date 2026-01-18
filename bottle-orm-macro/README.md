# Bottle ORM Macro

This crate provides the procedural macros for **Bottle ORM**, specifically the `#[derive(Model)]` macro. It is used to automatically generate schema definitions and SQL mapping for your Rust structs.

> **Note**: This crate is intended to be used as a dependency of `bottle-orm`, not standalone.

## Usage

Derive `Model` on your struct to register it with the ORM. You can customize field behavior using the `#[orm(...)]` attribute.

```rust
use bottle_orm::Model;

#[derive(Model)]
struct User {
    #[orm(primary_key)]
    id: i32,
    
    #[orm(size = 50, unique, index)]
    username: String,
    
    #[orm(create_time)]
    created_at: chrono::DateTime<chrono::Utc>,
}
```

## Supported Attributes

You can use the `#[orm(...)]` attribute on struct fields to configure database constraints and types.

| Attribute | Description | Example |
|-----------|-------------|---------|
| `primary_key` | Marks the column as the Primary Key. | `#[orm(primary_key)]` |
| `unique` | Adds a UNIQUE constraint to the column. | `#[orm(unique)]` |
| `index` | Creates a database index for this column. | `#[orm(index)]` |
| `create_time` | Sets the default value to the current timestamp (`NOW()`/`CURRENT_TIMESTAMP`). | `#[orm(create_time)]` |
| `size = N` | Sets the column size (e.g., `VARCHAR(N)`). Defaults to `TEXT` if omitted. | `#[orm(size = 255)]` |
| `foreign_key = "Table::Column"` | Defines a Foreign Key relationship. | `#[orm(foreign_key = "User::id")]` |

## Type Mapping

The macro automatically maps Rust types to SQL types:

| Rust Type | SQL Type (Postgres/General) |
|-----------|-----------------------------|
| `i32` | `INTEGER` |
| `i64` | `BIGINT` |
| `String` | `TEXT` (or `VARCHAR(n)` if `size` is set) |
| `bool` | `BOOLEAN` |
| `f64` | `DOUBLE PRECISION` |
| `Option<T>` | Nullable column of `T` |
| `chrono::DateTime<Utc>` | `TIMESTAMPTZ` |
| `chrono::NaiveDateTime` | `TIMESTAMP` |
| `chrono::NaiveDate` | `DATE` |
| `chrono::NaiveTime` | `TIME` |

## License

MIT
