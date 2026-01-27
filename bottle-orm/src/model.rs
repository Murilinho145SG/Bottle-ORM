//! # Model Module
//!
//! This module defines the core `Model` trait and associated structures for Bottle ORM.
//! It provides the interface that all database entities must implement, along with
//! metadata structures for describing table columns.
//!
//! ## Overview
//!
//! The `Model` trait is the foundation of Bottle ORM. It defines how Rust structs
//! map to database tables, including:
//!
//! - Table name resolution
//! - Column metadata (types, constraints, relationships)
//! - Serialization to/from database format
//!
//! ## Automatic Implementation
//!
//! The `Model` trait is typically implemented automatically via the `#[derive(Model)]`
//! procedural macro, which analyzes struct fields and `#[orm(...)]` attributes to
//! generate the necessary implementation.
//!
//! ## Example Usage
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
//!     #[orm(size = 100)]
//!     email: String,
//!
//!     age: Option<i32>,
//!
//!     #[orm(create_time)]
//!     created_at: DateTime<Utc>,
//! }
//!
//! #[derive(Model, Debug, Clone, Serialize, Deserialize, FromRow)]
//! struct Post {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!
//!     #[orm(foreign_key = "User::id")]
//!     user_id: Uuid,
//!
//!     #[orm(size = 200)]
//!     title: String,
//!
//!     content: String,
//!
//!     #[orm(create_time)]
//!     created_at: DateTime<Utc>,
//! }
//! ```
//!
//! ## Supported ORM Attributes
//!
//! - `#[orm(primary_key)]` - Marks field as primary key
//! - `#[orm(unique)]` - Adds UNIQUE constraint
//! - `#[orm(index)]` - Creates database index
//! - `#[orm(size = N)]` - Sets VARCHAR size (for String fields)
//! - `#[orm(create_time)]` - Auto-populate with current timestamp on creation
//! - `#[orm(update_time)]` - Auto-update timestamp on modification (future feature)
//! - `#[orm(foreign_key = "Table::Column")]` - Defines foreign key relationship

// ============================================================================
// External Crate Imports
// ============================================================================

use std::collections::HashMap;

// ============================================================================
// Column Metadata Structure
// ============================================================================

/// Metadata information about a database column.
///
/// This structure contains all the information needed to generate SQL table
/// definitions and handle type conversions between Rust and SQL. It is populated
/// automatically by the `#[derive(Model)]` macro based on struct field types
/// and `#[orm(...)]` attributes.
///
/// # Fields
///
/// * `name` - Column name (field name from struct)
/// * `sql_type` - SQL type string (e.g., "INTEGER", "TEXT", "UUID", "TIMESTAMPTZ")
/// * `is_primary_key` - Whether this is the primary key column
/// * `is_nullable` - Whether NULL values are allowed (from Option<T>)
/// * `create_time` - Auto-populate with CURRENT_TIMESTAMP on insert
/// * `update_time` - Auto-update timestamp on modification (future feature)
/// * `unique` - Whether UNIQUE constraint should be added
/// * `index` - Whether to create an index on this column
/// * `foreign_table` - Name of referenced table (for foreign keys)
/// * `foreign_key` - Name of referenced column (for foreign keys)
///
/// # Example
///
/// ```rust,ignore
/// // For this field:
/// #[orm(size = 50, unique, index)]
/// username: String,
///
/// // The generated ColumnInfo would be:
/// ColumnInfo {
///     name: "username",
///     sql_type: "VARCHAR(50)",
///     is_primary_key: false,
///     is_nullable: false,
///     create_time: false,
///     update_time: false,
///     unique: true,
///     index: true,
///     foreign_table: None,
///     foreign_key: None,
/// }
/// ```
///
/// # SQL Type Mapping
///
/// The `sql_type` field contains the SQL type based on the Rust type:
///
/// - `i32` → `"INTEGER"`
/// - `i64` → `"BIGINT"`
/// - `String` → `"TEXT"` or `"VARCHAR(N)"` with size attribute
/// - `bool` → `"BOOLEAN"`
/// - `f64` → `"DOUBLE PRECISION"`
/// - `Uuid` → `"UUID"`
/// - `DateTime<Utc>` → `"TIMESTAMPTZ"`
/// - `NaiveDateTime` → `"TIMESTAMP"`
/// - `NaiveDate` → `"DATE"`
/// - `NaiveTime` → `"TIME"`
/// - `Option<T>` → Same as T, but `is_nullable = true`
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// The column name in the database.
    ///
    /// This is derived from the struct field name and is typically converted
    /// to snake_case when generating SQL. The `r#` prefix is stripped if present
    /// (for Rust keywords used as field names).
    ///
    /// # Example
    /// ```rust,ignore
    /// // Field: user_id: i32
    /// name: "user_id"
    ///
    /// // Field: r#type: String (type is a Rust keyword)
    /// name: "r#type" // The r# will be stripped in SQL generation
    /// ```
    pub name: &'static str,

    /// The SQL type of the column (e.g., "TEXT", "INTEGER", "TIMESTAMPTZ").
    ///
    /// This string is used directly in CREATE TABLE statements. It must be
    /// a valid SQL type for the target database.
    ///
    /// # Example
    /// ```rust,ignore
    /// // i32 field
    /// sql_type: "INTEGER"
    ///
    /// // UUID field
    /// sql_type: "UUID"
    ///
    /// // String with size = 100
    /// sql_type: "VARCHAR(100)"
    /// ```
    pub sql_type: &'static str,

    /// Whether this column is a Primary Key.
    ///
    /// Set to `true` via `#[orm(primary_key)]` attribute. A table should have
    /// exactly one primary key column.
    ///
    /// # SQL Impact
    /// - Adds `PRIMARY KEY` constraint
    /// - Implicitly makes column `NOT NULL`
    /// - Creates a unique index automatically
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(primary_key)]
    /// id: Uuid,
    /// // is_primary_key: true
    /// ```
    pub is_primary_key: bool,

    /// Whether this column allows NULL values.
    ///
    /// Automatically set to `true` when the field type is `Option<T>`,
    /// otherwise `false` for non-optional types.
    ///
    /// # SQL Impact
    /// - `false`: Adds `NOT NULL` constraint
    /// - `true`: Allows NULL values
    ///
    /// # Example
    /// ```rust,ignore
    /// // Required field
    /// username: String,
    /// // is_nullable: false → NOT NULL
    ///
    /// // Optional field
    /// middle_name: Option<String>,
    /// // is_nullable: true → allows NULL
    /// ```
    pub is_nullable: bool,

    /// Whether this column should be automatically populated with the creation timestamp.
    ///
    /// Set via `#[orm(create_time)]` attribute. When `true`, the column gets
    /// a `DEFAULT CURRENT_TIMESTAMP` constraint.
    ///
    /// # SQL Impact
    /// - Adds `DEFAULT CURRENT_TIMESTAMP`
    /// - Column is auto-populated on INSERT
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(create_time)]
    /// created_at: DateTime<Utc>,
    /// // create_time: true
    /// // SQL: created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    /// ```
    pub create_time: bool,

    /// Whether this column should be automatically updated on modification.
    ///
    /// Set via `#[orm(update_time)]` attribute. This is a **future feature**
    /// not yet fully implemented.
    ///
    /// # Future Implementation
    /// When implemented, this will:
    /// - Add database trigger or application-level update
    /// - Auto-update timestamp on every UPDATE
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(update_time)]
    /// updated_at: DateTime<Utc>,
    /// // update_time: true (future feature)
    /// ```
    pub update_time: bool,

    /// Whether this column has a UNIQUE constraint.
    ///
    /// Set via `#[orm(unique)]` attribute. Ensures no two rows can have
    /// the same value in this column (NULL values may be exempt depending
    /// on database).
    ///
    /// # SQL Impact
    /// - Adds `UNIQUE` constraint
    /// - Creates a unique index automatically
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(unique)]
    /// username: String,
    /// // unique: true
    /// // SQL: username VARCHAR(255) UNIQUE
    /// ```
    pub unique: bool,

    /// Whether an index should be created for this column.
    ///
    /// Set via `#[orm(index)]` attribute. Creates a database index to speed
    /// up queries that filter or sort by this column.
    ///
    /// # SQL Impact
    /// - Creates separate `CREATE INDEX` statement
    /// - Index name: `idx_{table}_{column}`
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(index)]
    /// email: String,
    /// // index: true
    /// // SQL: CREATE INDEX idx_user_email ON user (email)
    /// ```
    pub index: bool,

    /// The name of the foreign table, if this is a Foreign Key.
    ///
    /// Set via `#[orm(foreign_key = "Table::Column")]` attribute. Contains
    /// the name of the referenced table.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(foreign_key = "User::id")]
    /// user_id: Uuid,
    /// // foreign_table: Some("User")
    /// ```
    pub foreign_table: Option<&'static str>,

    /// The name of the foreign column, if this is a Foreign Key.
    ///
    /// Set via `#[orm(foreign_key = "Table::Column")]` attribute. Contains
    /// the name of the referenced column in the foreign table.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(foreign_key = "User::id")]
    /// user_id: Uuid,
    /// // foreign_key: Some("id")
    /// // SQL: FOREIGN KEY (user_id) REFERENCES user (id)
    /// ```
    pub foreign_key: Option<&'static str>,

    /// Whether this field should be omitted from queries by default.
    ///
    /// Set via `#[orm(omit)]` attribute. When `true`, this column will be
    /// excluded from query results unless explicitly selected.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[orm(omit)]
    /// password: String,
    /// // omit: true
    /// // This field will not be included in SELECT * queries
    /// ```
    pub omit: bool,
}

// ============================================================================
// Model Trait
// ============================================================================

/// The core trait defining a Database Model (Table) in Bottle ORM.
///
/// This trait must be implemented by all structs that represent database tables.
/// It provides methods for retrieving table metadata, column information, and
/// converting instances to/from database format.
///
/// # Automatic Implementation
///
/// This trait is typically implemented automatically via the `#[derive(Model)]`
/// procedural macro. Manual implementation is possible but not recommended.
///
/// # Required Methods
///
/// * `table_name()` - Returns the table name
/// * `columns()` - Returns column metadata
/// * `active_columns()` - Returns column names
/// * `to_map()` - Serializes instance to a HashMap
///
/// # Example with Derive
///
/// ```rust,ignore
/// use bottle_orm::Model;
/// use uuid::Uuid;
///
/// #[derive(Model)]
/// struct User {
///     #[orm(primary_key)]
///     id: Uuid,
///     username: String,
///     age: i32,
/// }
///
/// // Now you can use:
/// assert_eq!(User::table_name(), "User");
/// assert_eq!(User::active_columns(), vec!["id", "username", "age"]);
/// ```
///
/// # Example Manual Implementation
///
/// ```rust,ignore
/// use bottle_orm::{Model, ColumnInfo};
/// use std::collections::HashMap;
///
/// struct CustomUser {
///     id: i32,
///     name: String,
/// }
///
/// impl Model for CustomUser {
///     fn table_name() -> &'static str {
///         "custom_users"
///     }
///
///     fn columns() -> Vec<ColumnInfo> {
///         vec![
///             ColumnInfo {
///                 name: "id",
///                 sql_type: "INTEGER",
///                 is_primary_key: true,
///                 is_nullable: false,
///                 create_time: false,
///                 update_time: false,
///                 unique: false,
///                 index: false,
///                 foreign_table: None,
///                 foreign_key: None,
///             },
///             ColumnInfo {
///                 name: "name",
///                 sql_type: "TEXT",
///                 is_primary_key: false,
///                 is_nullable: false,
///                 create_time: false,
///                 update_time: false,
///                 unique: false,
///                 index: false,
///                 foreign_table: None,
///                 foreign_key: None,
///             },
///         ]
///     }
///
///     fn active_columns() -> Vec<&'static str> {
///         vec!["id", "name"]
///     }
///
///     fn to_map(&self) -> HashMap<String, String> {
///         let mut map = HashMap::new();
///         map.insert("id".to_string(), self.id.to_string());
///         map.insert("name".to_string(), self.name.clone());
///         map
///     }
/// }
/// ```
pub trait Model {
    /// Returns the table name associated with this model.
    ///
    /// The table name is derived from the struct name and is used in all
    /// SQL queries. By default, the derive macro uses the struct name as-is,
    /// which is then converted to snake_case when generating SQL.
    ///
    /// # Returns
    ///
    /// A static string slice containing the table name
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Model)]
    /// struct UserProfile {
    ///     // ...
    /// }
    ///
    /// // Returns "UserProfile"
    /// // SQL will use: "user_profile" (snake_case)
    /// assert_eq!(UserProfile::table_name(), "UserProfile");
    /// ```
    fn table_name() -> &'static str;

    /// Returns the list of column definitions for this model.
    ///
    /// This method provides complete metadata about each column, including
    /// SQL types, constraints, and relationships. The information is used
    /// for table creation, query building, and type conversion.
    ///
    /// # Returns
    ///
    /// A vector of `ColumnInfo` structs describing each column
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Model)]
    /// struct User {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     username: String,
    /// }
    ///
    /// let columns = User::columns();
    /// assert_eq!(columns.len(), 2);
    /// assert!(columns[0].is_primary_key);
    /// assert_eq!(columns[1].sql_type, "TEXT");
    /// ```
    fn columns() -> Vec<ColumnInfo>;

    /// Returns the names of active columns (struct fields).
    ///
    /// This method returns a simple list of column names without metadata.
    /// It's used for query building and SELECT statement generation.
    ///
    /// # Returns
    ///
    /// A vector of static string slices containing column names
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Model)]
    /// struct User {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     username: String,
    ///     email: String,
    /// }
    ///
    /// assert_eq!(
    ///     User::active_columns(),
    ///     vec!["id", "username", "email"]
    /// );
    /// ```
    fn active_columns() -> Vec<&'static str>;

    /// Converts the model instance into a value map (Column Name → String Value).
    ///
    /// This method serializes the model instance into a HashMap where keys are
    /// column names and values are string representations. It's used primarily
    /// for INSERT operations.
    ///
    /// # Returns
    ///
    /// A HashMap mapping column names to string values
    ///
    /// # Type Conversion
    ///
    /// All values are converted to strings via the `ToString` trait:
    /// - Primitives: Direct conversion (e.g., `42` → `"42"`)
    /// - UUID: Hyphenated format (e.g., `"550e8400-e29b-41d4-a716-446655440000"`)
    /// - DateTime: RFC 3339 format
    /// - Option<T>: Only included if Some, omitted if None
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use uuid::Uuid;
    ///
    /// #[derive(Model)]
    /// struct User {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     username: String,
    ///     age: i32,
    /// }
    ///
    /// let user = User {
    ///     id: Uuid::new_v4(),
    ///     username: "john_doe".to_string(),
    ///     age: 25,
    /// };
    ///
    /// let map = user.to_map();
    /// assert!(map.contains_key("id"));
    /// assert_eq!(map.get("username"), Some(&"john_doe".to_string()));
    /// assert_eq!(map.get("age"), Some(&"25".to_string()));
    /// ```
    fn to_map(&self) -> HashMap<String, String>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_info_creation() {
        let col = ColumnInfo {
            name: "test_column",
            sql_type: "INTEGER",
            is_primary_key: true,
            is_nullable: false,
            create_time: false,
            update_time: false,
            unique: false,
            index: false,
            foreign_table: None,
            foreign_key: None,
            omit: false,
        };

        assert_eq!(col.name, "test_column");
        assert_eq!(col.sql_type, "INTEGER");
        assert!(col.is_primary_key);
        assert!(!col.is_nullable);
    }

    #[test]
    fn test_column_info_with_foreign_key() {
        let col = ColumnInfo {
            name: "user_id",
            sql_type: "UUID",
            is_primary_key: false,
            is_nullable: false,
            create_time: false,
            update_time: false,
            unique: false,
            index: false,
            foreign_table: Some("User"),
            foreign_key: Some("id"),
            omit: false,
        };

        assert_eq!(col.foreign_table, Some("User"));
        assert_eq!(col.foreign_key, Some("id"));
    }
}
