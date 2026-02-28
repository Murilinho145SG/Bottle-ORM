//! # Bottle ORM Procedural Macros
//!
//! This crate provides procedural macros for the Bottle ORM framework.
//! It contains the `#[derive(Model)]` macro that automatically implements
//! the `Model` trait for structs representing database tables.
//!
//! ## Overview
//!
//! The procedural macros in this crate analyze struct definitions and their
//! attributes to generate the necessary boilerplate code for ORM functionality.
//! This includes:
//!
//! - Table name resolution
//! - Column metadata generation
//! - Type mapping from Rust to SQL
//! - Serialization methods
//!
//! ## Architecture
//!
//! The crate is organized into three modules:
//!
//! - **`lib.rs`** (this file): Entry point and macro definitions
//! - **`derive_model.rs`**: Implementation of the Model derive macro
//! - **`types.rs`**: Type mapping utilities (Rust → SQL)
//!
//! ## Usage
//!
//! This crate is not meant to be used directly. Instead, it's re-exported
//! by the main `bottle-orm` crate:
//!
//! ```rust,ignore
//! use bottle_orm::Model;
//! use uuid::Uuid;
//!
//! #[derive(Model)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!     username: String,
//! }
//! ```
//!
//! ## Supported Attributes
//!
//! The `#[orm(...)]` attribute supports the following options:
//!
//! ### Primary Key
//! ```rust,ignore
//! #[orm(primary_key)]
//! id: Uuid,
//! ```
//! Marks the field as the table's primary key. Generates `PRIMARY KEY` constraint.
//!
//! ### Unique Constraint
//! ```rust,ignore
//! #[orm(unique)]
//! username: String,
//! ```
//! Adds a `UNIQUE` constraint to prevent duplicate values.
//!
//! ### Database Index
//! ```rust,ignore
//! #[orm(index)]
//! email: String,
//! ```
//! Creates a database index on the column for faster queries.
//!
//! ### Column Size
//! ```rust,ignore
//! #[orm(size = 100)]
//! username: String,
//! ```
//! Sets `VARCHAR(N)` size for String fields. Default is `TEXT`.
//!
//! ### Auto-Timestamp (Creation)
//! ```rust,ignore
//! #[orm(create_time)]
//! created_at: DateTime<Utc>,
//! ```
//! Adds `DEFAULT CURRENT_TIMESTAMP` to auto-populate on INSERT.
//!
//! ### Auto-Timestamp (Update)
//! ```rust,ignore
//! #[orm(update_time)]
//! updated_at: DateTime<Utc>,
//! ```
//! Auto-updates timestamp on UPDATE (future feature).
//!
//! ### Foreign Key
//! ```rust,ignore
//! #[orm(foreign_key = "User::id")]
//! user_id: Uuid,
//! ```
//! Creates a foreign key relationship. Format: `"TargetTable::target_column"`.
//!
//! ### Omit Field
//! ```rust,ignore
//! #[orm(omit)]
//! password: String,
//! ```
//! Excludes this field from query results by default. Returns a placeholder value
//! instead of the actual data (`"omited"` for strings, `1970-01-01T00:00:00Z` for dates, etc.).
//!
//! ### Combining Attributes
//! ```rust,ignore
//! #[orm(size = 50, unique, index)]
//! username: String,
//! ```
//! Multiple attributes can be combined on a single field.
//!
//! ## Generated Field Constants
//!
//! The macro also generates a `{model}_fields` module with constants for each field,
//! enabling IDE autocomplete:
//!
//! ```rust,ignore
//! // For struct User, the macro generates:
//! pub mod user_fields {
//!     pub const ID: &'static str = "id";
//!     pub const USERNAME: &'static str = "username";
//!     pub const PASSWORD: &'static str = "password";
//! }
//!
//! // Use with filter, select, omit, etc:
//! db.model::<User>()
//!     .filter(user_fields::AGE, ">=", 18)
//!     .omit(user_fields::PASSWORD)
//!     .scan()
//!     .await?;
//! ```
//!
//! ## Type Support
//!
//! The macro supports automatic type mapping for:
//!
//! ### Primitive Types
//! - `i32` → `INTEGER`
//! - `i64` → `BIGINT`
//! - `f64` → `DOUBLE PRECISION`
//! - `bool` → `BOOLEAN`
//! - `String` → `TEXT` or `VARCHAR(N)`
//!
//! ### UUID Types (All Versions 1-7)
//! - `Uuid` → `UUID`
//!
//! ### Date/Time Types
//! - `DateTime<Utc>` → `TIMESTAMPTZ`
//! - `NaiveDateTime` → `TIMESTAMP`
//! - `NaiveDate` → `DATE`
//! - `NaiveTime` → `TIME`
//!
//! ### Nullable Types
//! - `Option<T>` → SQL type of `T` with `NULL` allowed
//!
//! ## Complete Example
//!
//! ```rust,ignore
//! use bottle_orm::Model;
//! use uuid::Uuid;
//! use chrono::{DateTime, Utc};
//! use serde::{Deserialize, Serialize};
//! use sqlx::FromRow;
//!
//! #[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!
//!     #[orm(size = 50, unique, index)]
//!     username: String,
//!
//!     #[orm(size = 100, unique)]
//!     email: String,
//!
//!     age: Option<i32>,
//!
//!     active: bool,
//!
//!     #[orm(create_time)]
//!     created_at: DateTime<Utc>,
//!
//!     #[orm(update_time)]
//!     updated_at: Option<DateTime<Utc>>,
//! }
//!
//! #[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
//! struct Post {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!
//!     #[orm(foreign_key = "User::id", index)]
//!     user_id: Uuid,
//!
//!     #[orm(size = 200)]
//!     title: String,
//!
//!     content: String,
//!
//!     published: bool,
//!
//!     #[orm(create_time)]
//!     created_at: DateTime<Utc>,
//! }
//! ```

// ============================================================================
// Compiler Directives
// ============================================================================

// Ensure this crate is only used as a proc-macro crate
#![warn(missing_docs)]

// ============================================================================
// External Crate Imports
// ============================================================================

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

// ============================================================================
// Module Declarations
// ============================================================================

/// Type mapping module - converts Rust types to SQL types.
///
/// This module contains the logic for mapping Rust types (including primitives,
/// UUID, chrono types, and Option<T>) to their corresponding SQL type strings.
mod types;

/// Model derive implementation module.
///
/// This module contains the core logic for expanding the `#[derive(Model)]`
/// macro, including attribute parsing and code generation.
mod derive_model;

/// Enum derive implementation module.
mod derive_enum;

/// FromAnyRow derive implementation module.
mod derive_anyrow;

// ============================================================================
// Procedural Macro Definitions
// ============================================================================

/// Derives the `Model` trait for a struct.
///
/// This procedural macro inspects the struct fields and generates the necessary
/// code to map the struct to a database table. It automatically implements the
/// `Model` trait with methods for retrieving table metadata and converting
/// instances to/from database format.
///
/// # Supported Attributes
///
/// The macro recognizes the following `#[orm(...)]` attributes on struct fields:
///
/// * `primary_key` - Marks the field as a primary key
/// * `unique` - Adds a UNIQUE constraint
/// * `index` - Creates a database index
/// * `create_time` - Sets default value to CURRENT_TIMESTAMP
/// * `update_time` - Auto-updates timestamp (future feature)
/// * `size = N` - Sets column size (VARCHAR(N))
/// * `foreign_key = "Table::Column"` - Defines a Foreign Key relationship
/// * `omit` - Excludes field from queries (returns placeholder value)
///
/// # Type Mapping
///
/// The macro automatically maps Rust types to SQL types:
///
/// - **Primitives**: `i32` → INTEGER, `i64` → BIGINT, `bool` → BOOLEAN, etc.
/// - **UUID**: `Uuid` → UUID (supports all versions 1-7)
/// - **Strings**: `String` → TEXT or VARCHAR(N) with size attribute
/// - **Date/Time**: `DateTime<Utc>` → TIMESTAMPTZ, etc.
/// - **Nullable**: `Option<T>` → SQL type of T with NULL allowed
///
/// # Requirements
///
/// The struct must have named fields. Tuple structs and unit structs are not supported.
///
/// # Generated Implementation
///
/// The macro generates an implementation of the `Model` trait with four methods:
///
/// 1. `table_name()` - Returns the struct name as a static string
/// 2. `columns()` - Returns column metadata as `Vec<ColumnInfo>`
/// 3. `active_columns()` - Returns column names as `Vec<&'static str>`
/// 4. `to_map()` - Serializes the instance to `HashMap<String, String>`
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::Model;
/// use uuid::Uuid;
/// use chrono::{DateTime, Utc};
///
/// #[derive(Model)]
/// struct User {
///     #[orm(primary_key)]
///     id: Uuid,
///
///     #[orm(size = 50, unique)]
///     username: String,
///
///     #[orm(size = 100)]
///     email: String,
///
///     age: i32,
///
///     #[orm(create_time)]
///     created_at: DateTime<Utc>,
/// }
/// ```
///
/// # Panics
///
/// The macro will panic at compile time if:
///
/// - The input is not a struct
/// - The struct doesn't have named fields
/// - An `#[orm(...)]` attribute is malformed
/// - A `foreign_key` attribute doesn't follow the "Table::Column" format
///
/// # See Also
///
/// * [`Model`](../bottle_orm/trait.Model.html) - The trait being implemented
/// * [`ColumnInfo`](../bottle_orm/struct.ColumnInfo.html) - Column metadata structure
#[proc_macro_derive(Model, attributes(orm))]
pub fn model_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let ast = parse_macro_input!(input as DeriveInput);

    // Expand the macro to generate the Model trait implementation
    let expanded = derive_model::expand(ast);

    // Convert the generated code back into a TokenStream
    TokenStream::from(expanded)
}

/// Derives `Display` and `FromStr` for an enum to facilitate database mapping.
///
/// This macro generates implementations that allow the enum to be easily saved
/// as a string (via `to_string()`) and loaded back from a string (via `parse()`).
/// It uses the variant names as the string representation.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(BottleEnum, Debug, Clone, PartialEq)]
/// enum UserStatus {
///     Active,
///     Inactive,
///     Pending,
/// }
/// ```
#[proc_macro_derive(BottleEnum)]
pub fn enum_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let expanded = derive_enum::expand(ast);
    TokenStream::from(expanded)
}

/// Derives the `FromRow` trait for `AnyRow` and the `AnyImpl` trait.
///
/// This procedural macro generates an implementation of `sqlx::FromRow<'r, sqlx::any::AnyRow>`
/// for the target struct, allowing it to be scanned directly from database results when
/// using `sqlx::Any` driver (which Bottle ORM uses internally).
///
/// It also implements the `AnyImpl` trait, which provides necessary column metadata used
/// by the `QueryBuilder` for dynamic query construction.
///
/// # Features
///
/// - **Automatic Field Mapping**: Maps database columns to struct fields by name.
/// - **DateTime Handling**: Includes special logic to handle `DateTime` types, often required
///   when dealing with the `Any` driver's type erasure or JSON serialization fallback.
/// - **Metadata Generation**: Automatically generates `AnyInfo` for each field.
///
/// # Requirements
///
/// The struct must have named fields. Tuple structs and unit structs are not supported.
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::{FromAnyRow, AnyImpl};
/// use chrono::{DateTime, Utc};
///
/// #[derive(FromAnyRow)]
/// struct UserCount {
///     count: i64,
///     last_active: DateTime<Utc>,
/// }
///
/// // Usage with QueryBuilder:
/// // let stats: UserCount = db.model::<User>().select("count(*), last_active").first().await?;
/// ```
#[proc_macro_derive(FromAnyRow)]
pub fn any_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let expanded = derive_anyrow::expand(ast);
    TokenStream::from(expanded)
}
