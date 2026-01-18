# Bottle ORM Macro

[![Crates.io](https://img.shields.io/crates/v/bottle-orm-macro.svg)](https://crates.io/crates/bottle-orm-macro)
[![Docs.rs](https://docs.rs/bottle-orm-macro/badge.svg)](https://docs.rs/bottle-orm-macro)
[![License](https://img.shields.io/crates/l/bottle-orm-macro.svg)](https://github.com/Murilinho145SG/bottle-orm/blob/main/LICENSE)

This crate provides the procedural macros for **[Bottle ORM](https://crates.io/crates/bottle-orm)**, specifically the `#[derive(Model)]` macro. It is used to automatically generate schema definitions and SQL mapping for your Rust structs.

> **Note**: This crate is intended to be used as a dependency of `bottle-orm`, not standalone. Please refer to the [main bottle-orm documentation](https://crates.io/crates/bottle-orm) for usage instructions.

## Installation

You should install the main crate:

```toml
[dependencies]
bottle-orm = "0.1.0"
```

However, if you need to inspect this macro crate specifically:

```toml
[dependencies]
bottle-orm-macro = "0.1.0"
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

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.