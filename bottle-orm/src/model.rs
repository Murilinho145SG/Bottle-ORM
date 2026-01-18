use std::collections::HashMap;

/// Metadata information about a database column.
///
/// This structure is used internally to generate table schemas and map Rust types to SQL types.
/// It is usually populated automatically by the `#[derive(Model)]` macro.
pub struct ColumnInfo {
    /// The column name in the database.
    pub name: &'static str,
    /// The SQL type of the column (e.g., "TEXT", "INTEGER", "TIMESTAMPTZ").
    pub sql_type: &'static str,
    /// Whether this column is a Primary Key.
    pub is_primary_key: bool,
    /// Whether this column allows NULL values.
    pub is_nullable: bool,
    /// Whether this column should be automatically populated with the creation timestamp.
    pub create_time: bool,
    /// Whether this column should be automatically updated on modification (feature in progress).
    pub update_time: bool,
    /// Whether this column has a UNIQUE constraint.
    pub unique: bool,
    /// Whether an index should be created for this column.
    pub index: bool,
    /// The name of the foreign table, if this is a Foreign Key.
    pub foreign_table: Option<&'static str>,
    /// The name of the foreign column, if this is a Foreign Key.
    pub foreign_key: Option<&'static str>,
}

/// The core trait defining a Database Model (Table) in Bottle ORM.
///
/// This trait is typically implemented automatically via the `#[derive(Model)]` macro.
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::Model;
///
/// #[derive(Model)]
/// struct User {
///     #[orm(primary_key)]
///     id: i32,
///     username: String,
/// }
/// ```
pub trait Model {
    /// Returns the table name associated with this model.
    /// usually converted from CamelCase struct name to snake_case.
    fn table_name() -> &'static str;
    
    /// Returns the list of column definitions for this model.
    fn columns() -> Vec<ColumnInfo>;
    
    /// Returns the names of active columns (struct fields).
    fn active_columns() -> Vec<&'static str>;
    
    /// Converts the model instance into a value map (Column Name -> String Value).
    /// Used primarily for INSERT operations.
    fn to_map(&self) -> HashMap<String, String>;
}