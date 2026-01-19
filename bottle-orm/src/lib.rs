//! # Bottle ORM
//!
//! **Bottle ORM** is a lightweight, async ORM for Rust built on top of [sqlx](https://github.com/launchbadge/sqlx).
//! It is designed to be simple, efficient, and easy to use, providing a fluent Query Builder
//! and automatic schema migrations.
//!
//! ## Features
//!
//! - **Async & Non-blocking**: Built on `tokio` and `sqlx`
//! - **Multi-Driver Support**: Compatible with PostgreSQL, MySQL, and SQLite (via `sqlx::Any`)
//! - **Macro-based Models**: Define your schema using standard Rust structs with `#[derive(Model)]`
//! - **Fluent Query Builder**: Chainable methods for filtering, selecting, pagination, and sorting
//! - **Auto-Migration**: Automatically creates tables and foreign key constraints based on your structs
//! - **UUID Support**: Full support for UUID versions 1 through 7
//!
//! ## Quick Start Example
//!
//! ```rust,ignore
//! use bottle_orm::{Database, Model};
//! use serde::{Deserialize, Serialize};
//! use sqlx::FromRow;
//!
//! #[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: i32,
//!     #[orm(size = 50, unique)]
//!     username: String,
//!     age: i32,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = Database::connect("sqlite::memory:").await?;
//!
//!     db.migrator()
//!         .register::<User>()
//!         .run()
//!         .await?;
//!
//!     let users: Vec<User> = db.model::<User>()
//!         .filter("age", ">=", 18)
//!         .scan()
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

// ============================================================================
// Macro Re-exports
// ============================================================================

/// Re-export of the procedural macro for deriving the `Model` trait.
///
/// This macro is defined in the `bottle-orm-macro` crate and allows automatic
/// implementation of the `Model` trait for structs representing database tables.
pub use bottle_orm_macro::Model;

// ============================================================================
// Module Declarations
// ============================================================================

/// Database connection and driver management.
///
/// Contains the `Database` struct for connection pooling and driver detection,
/// as well as the `Drivers` enum for identifying the database backend.
pub mod database;

/// Core Model trait and column metadata structures.
///
/// Defines the `Model` trait that must be implemented by all ORM entities,
/// and the `ColumnInfo` struct containing metadata about table columns.
pub mod model;

/// Fluent query builder for constructing SQL queries.
///
/// Provides the `QueryBuilder` struct with chainable methods for building
/// SELECT, INSERT, and filtered queries with type-safe parameter binding.
pub mod query_builder;

/// Schema migration management.
///
/// Contains the `Migrator` struct for registering models and executing
/// automatic table creation and foreign key assignment.
pub mod migration;

/// Error types and handling.
///
/// Defines the `Error` enum with variants for different error scenarios
/// that can occur during ORM operations.
pub mod errors;

/// Temporal type conversion and handling.
///
/// Provides specialized conversion functions for chrono types (DateTime, NaiveDateTime, etc.)
/// across different database drivers, optimizing for native database type support.
pub mod temporal;

/// Value binding utilities for SQL queries.
///
/// Provides type-safe value binding with automatic type detection and conversion,
/// supporting all SQL types across different database drivers.
pub mod value_binding;

// ============================================================================
// Public API Re-exports
// ============================================================================

/// Re-export of the `Database` struct for connection management.
///
/// This is the main entry point for establishing database connections
/// and creating query builders or migrators.
pub use database::Database;

/// Re-export of the `Model` trait and `ColumnInfo` struct.
///
/// The `Model` trait defines the interface for ORM entities, while
/// `ColumnInfo` contains metadata about individual table columns.
pub use model::{ColumnInfo, Model};

/// Re-export of the `QueryBuilder` for constructing and executing queries.
///
/// `QueryBuilder` provides a fluent interface for building SELECT and INSERT
/// queries with filtering, ordering, and pagination capabilities.
pub use query_builder::QueryBuilder;

/// Re-export of the `Migrator` for schema migration management.
///
/// `Migrator` handles the registration of models and execution of
/// migration tasks to create tables and establish relationships.
pub use migration::Migrator;

/// Re-export of the `Error` type for error handling.
///
/// This is the main error type used throughout Bottle ORM, wrapping
/// various error scenarios including database errors and validation errors.
pub use errors::Error;
