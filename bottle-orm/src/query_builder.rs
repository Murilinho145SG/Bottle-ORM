//! # Query Builder Module
//!
//! This module provides a fluent interface for constructing and executing SQL queries.
//! It handles SELECT, INSERT, filtering (WHERE), pagination (LIMIT/OFFSET), and ordering operations
//! with type-safe parameter binding across different database drivers.
//!
//! ## Features
//!
//! - **Fluent API**: Chainable methods for building complex queries
//! - **Type-Safe Binding**: Automatic parameter binding with support for multiple types
//! - **Multi-Driver Support**: Works with PostgreSQL, MySQL, and SQLite
//! - **UUID Support**: Full support for UUID versions 1-7
//! - **Pagination**: Built-in LIMIT/OFFSET support with helper methods
//! - **Custom Filters**: Support for manual SQL construction with closures
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bottle_orm::{Database, Model};
//! use uuid::Uuid;
//!
//! // Simple query
//! let users: Vec<User> = db.model::<User>()
//!     .filter("age", ">=", 18)
//!     .order("created_at DESC")
//!     .limit(10)
//!     .scan()
//!     .await?;
//!
//! // Query with UUID filter
//! let user_id = Uuid::new_v4();
//! let user: User = db.model::<User>()
//!     .filter("id", "=", user_id)
//!     .first()
//!     .await?;
//!
//! // Insert a new record
//! let new_user = User {
//!     id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
//!     username: "john_doe".to_string(),
//!     age: 25,
//! };
//! db.model::<User>().insert(&new_user).await?;
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use heck::ToSnakeCase;
use sqlx::{
    any::{AnyArguments, AnyRow},
    Any, Arguments, Decode, Encode, FromRow, Row, Type,
};
use std::{fmt::format, marker::PhantomData};
use uuid::Uuid;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{
    database::{Connection, Drivers},
    model::{ColumnInfo, Model},
    temporal::{self, is_temporal_type},
    value_binding::ValueBinder,
    AnyImpl, Error,
};

// ============================================================================
// Type Aliases
// ============================================================================

/// A type alias for filter closures that support manual SQL construction and argument binding.
///
/// Filter functions receive the following parameters:
/// 1. `&mut String` - The SQL query buffer being built
/// 2. `&mut AnyArguments` - The argument container for binding values
/// 3. `&Drivers` - The current database driver (determines placeholder syntax)
/// 4. `&mut usize` - The argument counter (for PostgreSQL `$n` placeholders)
///
/// ## Example
///
/// ```rust,ignore
/// let custom_filter: FilterFn = Box::new(|query, args, driver, counter| {
///     query.push_str(" AND age > ");
///     match driver {
///         Drivers::Postgres => {
///             query.push_str(&format!("${}", counter));
///             *counter += 1;
///         }
///         _ => query.push('?'),
///     }
///     args.add(18);
/// });
/// ```
pub type FilterFn = Box<dyn Fn(&mut String, &mut AnyArguments<'_>, &Drivers, &mut usize) + Send + Sync>;

// ============================================================================
// QueryBuilder Struct
// ============================================================================

/// A fluent Query Builder for constructing SQL queries.
///
/// `QueryBuilder` provides a type-safe, ergonomic interface for building and executing
/// SQL queries across different database backends. It supports filtering, ordering,
/// pagination, and both SELECT and INSERT operations.
///
/// ## Type Parameter
///
/// * `'a` - Lifetime of the database reference (used for PhantomData)
/// * `T` - The Model type this query operates on
/// * `E` - The connection type (Database or Transaction)
///
/// ## Fields
///
/// * `db` - Reference to the database connection pool or transaction
/// * `table_name` - Static string containing the table name
/// * `columns_info` - Metadata about each column in the table
/// * `columns` - List of column names in snake_case format
/// * `select_columns` - Specific columns to select (empty = SELECT *)
/// * `where_clauses` - List of filter functions to apply
/// * `order_clauses` - List of ORDER BY clauses
/// * `limit` - Maximum number of rows to return
/// * `offset` - Number of rows to skip (for pagination)
/// * `_marker` - PhantomData to bind the generic type T
pub struct QueryBuilder<'a, T, E>
where
    E: Connection,
{
    /// Reference to the database connection pool
    pub(crate) tx: E,

    /// Name of the database table (in original case)
    pub(crate) table_name: &'static str,

    /// Metadata information about each column
    pub(crate) columns_info: Vec<ColumnInfo>,

    /// List of column names (in snake_case)
    pub(crate) columns: Vec<String>,

    /// Specific columns to select (empty means SELECT *)
    pub(crate) select_columns: Vec<String>,

    /// Collection of WHERE clause filter functions
    pub(crate) where_clauses: Vec<FilterFn>,

    /// Collection of ORDER BY clauses
    pub(crate) order_clauses: Vec<String>,

    /// Collection of JOIN clause to filter entry tables
    pub(crate) joins_clauses: Vec<String>,

    /// Maximum number of rows to return (LIMIT)
    pub(crate) limit: Option<usize>,

    /// Number of rows to skip (OFFSET)
    pub(crate) offset: Option<usize>,

    /// PhantomData to bind the generic type T
    pub(crate) _marker: PhantomData<&'a T>,
}

// ============================================================================
// QueryBuilder Implementation
// ============================================================================

impl<'a, T, E> QueryBuilder<'a, T, E>
where
    T: Model + Send + Sync + Unpin,
    E: Connection,
{
    // ========================================================================
    // Constructor
    // ========================================================================

    /// Creates a new QueryBuilder instance.
    ///
    /// This constructor is typically called internally via `db.model::<T>()`.
    /// You rarely need to call this directly.
    ///
    /// # Arguments
    ///
    /// * `db` - Reference to the database connection
    /// * `table_name` - Name of the table to query
    /// * `columns_info` - Metadata about table columns
    /// * `columns` - List of column names
    ///
    /// # Returns
    ///
    /// A new `QueryBuilder` instance ready for query construction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Usually called via db.model::<User>()
    /// let query = db.model::<User>();
    /// ```
    pub fn new(tx: E, table_name: &'static str, columns_info: Vec<ColumnInfo>, columns: Vec<String>) -> Self {
        Self {
            tx,
            table_name,
            columns_info,
            columns,
            select_columns: Vec::new(),
            where_clauses: Vec::new(),
            order_clauses: Vec::new(),
            joins_clauses: Vec::new(),
            limit: None,
            offset: None,
            _marker: PhantomData,
        }
    }

    // ========================================================================
    // Query Building Methods
    // ========================================================================

    /// Adds a WHERE clause to the query.
    ///
    /// This method adds a filter condition to the query. Multiple filters can be chained
    /// and will be combined with AND operators. The value is bound as a parameter to
    /// prevent SQL injection.
    ///
    /// # Type Parameters
    ///
    /// * `V` - The type of the value to filter by. Must be encodable for SQL queries.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to filter on
    /// * `op` - The comparison operator (e.g., "=", ">", "LIKE", "IN")
    /// * `value` - The value to compare against
    ///
    /// # Supported Types
    ///
    /// - Primitives: `i32`, `i64`, `f64`, `bool`, `String`
    /// - UUID: `Uuid` (all versions 1-7)
    /// - Date/Time: `DateTime<Utc>`, `NaiveDateTime`, `NaiveDate`, `NaiveTime`
    /// - Options: `Option<T>` for any supported type T
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Filter by integer
    /// query.filter("age", ">=", 18)
    ///
    /// // Filter by string
    /// query.filter("username", "=", "john_doe")
    ///
    /// // Filter by UUID
    /// let user_id = Uuid::new_v4();
    /// query.filter("id", "=", user_id)
    ///
    /// // Filter with LIKE operator
    /// query.filter("email", "LIKE", "%@example.com")
    ///
    /// // Chain multiple filters
    /// query
    ///     .filter("age", ">=", 18)
    ///     .filter("active", "=", true)
    ///     .filter("role", "=", "admin")
    /// ```
    pub fn filter<V>(mut self, col: &'static str, op: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(" ");
            query.push_str(op);
            query.push(' ');

            // Handle different placeholder syntaxes based on database driver
            match driver {
                // PostgreSQL uses numbered placeholders: $1, $2, $3, ...
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    *arg_counter += 1;
                }
                // MySQL and SQLite use question mark placeholders: ?
                _ => query.push('?'),
            }

            // Bind the value to the query
            let _ = args.add(value.clone());
        });

        self.where_clauses.push(clause);
        self
    }

    /// Adds an equality filter to the query.
    ///
    /// This is a convenience wrapper around `filter()` for simple equality checks.
    /// It is equivalent to calling `filter(col, "=", value)`.
    ///
    /// # Type Parameters
    ///
    /// * `V` - The type of the value to compare against.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to filter on.
    /// * `value` - The value to match.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Equivalent to filter("age", "=", 18)
    /// query.equals("age", 18)
    /// ```
    pub fn equals<V>(self, col: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter(col, "=", value)
    }

    /// Adds an ORDER BY clause to the query.
    ///
    /// Specifies the sort order for the query results. Multiple order clauses
    /// can be added and will be applied in the order they were added.
    ///
    /// # Arguments
    ///
    /// * `order` - The ORDER BY expression (e.g., "created_at DESC", "age ASC, name DESC")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Single column ascending (ASC is default)
    /// query.order("age")
    ///
    /// // Single column descending
    /// query.order("created_at DESC")
    ///
    /// // Multiple columns
    /// query.order("age DESC, username ASC")
    ///
    /// // Chain multiple order clauses
    /// query
    ///     .order("priority DESC")
    ///     .order("created_at ASC")
    /// ```
    pub fn order(mut self, order: &str) -> Self {
        self.order_clauses.push(order.to_string());
        self
    }

    /// Placeholder for eager loading relationships (preload).
    ///
    /// This method is reserved for future implementation of relationship preloading.
    /// Currently, it returns `self` unchanged to maintain the fluent interface.
    ///
    /// # Future Implementation
    ///
    /// Will support eager loading of related models to avoid N+1 query problems:
    ///
    /// ```rust,ignore
    /// // Future usage example
    /// query.preload("posts").preload("comments")
    /// ```
    pub fn preload(self) -> Self {
        // TODO: Implement relationship preloading
        self
    }

    /// Placeholder for JOIN operations.
    ///
    /// This method is reserved for future implementation of SQL JOINs.
    /// Currently, it returns `self` unchanged to maintain the fluent interface.
    ///
    /// # Future Implementation
    ///
    /// Will support various types of JOINs (INNER, LEFT, RIGHT, FULL):
    ///
    /// ```rust,ignore
    /// Adds a JOIN clause to the query.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join.
    /// * `s_query` - The ON clause condition (e.g., "users.id = posts.user_id").
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.join("posts", "users.id = posts.user_id")
    /// ```
    pub fn join(mut self, table: &str, s_query: &str) -> Self {
        self.joins_clauses.push(format!("JOIN {} ON {}", table, s_query));
        self
    }

    /// Applies pagination with validation and limits.
    ///
    /// This is a convenience method that combines `limit()` and `offset()` with
    /// built-in validation and maximum value enforcement for safer pagination.
    ///
    /// # Arguments
    ///
    /// * `max_value` - Maximum allowed items per page
    /// * `default` - Default value if `value` exceeds `max_value`
    /// * `page` - Zero-based page number
    /// * `value` - Requested items per page
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` - The updated QueryBuilder with pagination applied
    /// * `Err(Error)` - If `value` is negative
    ///
    /// # Pagination Logic
    ///
    /// 1. Validates that `value` is non-negative
    /// 2. If `value` > `max_value`, uses `default` instead
    /// 3. Calculates offset as: `value * page`
    /// 4. Sets limit to `value`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Page 0 with 10 items (page 1 in 1-indexed systems)
    /// query.pagination(100, 20, 0, 10)?  // LIMIT 10 OFFSET 0
    ///
    /// // Page 2 with 25 items (page 3 in 1-indexed systems)
    /// query.pagination(100, 20, 2, 25)?  // LIMIT 25 OFFSET 50
    ///
    /// // Request too many items, falls back to default
    /// query.pagination(100, 20, 0, 150)? // LIMIT 20 OFFSET 0 (150 > 100)
    ///
    /// // Error: negative value
    /// query.pagination(100, 20, 0, -10)? // Returns Error
    /// ```
    pub fn pagination(mut self, max_value: usize, default: usize, page: usize, value: isize) -> Result<Self, Error> {
        // Validate that value is non-negative
        if value < 0 {
            return Err(Error::InvalidArgument("value cannot be negative".into()));
        }

        let mut f_value = value as usize;

        // Enforce maximum value limit
        if f_value > max_value {
            f_value = default;
        }

        // Apply offset and limit
        self = self.offset(f_value * page);
        self = self.limit(f_value);

        Ok(self)
    }

    /// Selects specific columns to return.
    ///
    /// By default, queries use `SELECT *` to return all columns. This method
    /// allows you to specify exactly which columns should be returned, which can
    /// improve performance for tables with many or large columns.
    ///
    /// # Arguments
    ///
    /// * `columns` - Comma-separated list of column names to select
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Select single column
    /// query.select("id")
    ///
    /// // Select multiple columns
    /// query.select("id, username, email")
    ///
    /// // Select with SQL functions
    /// query.select("COUNT(*) as total")
    ///
    /// // Chain multiple select calls (all will be included)
    /// query
    ///     .select("id, username")
    ///     .select("created_at")
    /// ```
    pub fn select(mut self, columns: &str) -> Self {
        self.select_columns.push(columns.to_string().to_snake_case());
        self
    }

    /// Sets the query offset (pagination).
    ///
    /// Specifies the number of rows to skip before starting to return rows.
    /// Commonly used in combination with `limit()` for pagination.
    ///
    /// # Arguments
    ///
    /// * `offset` - Number of rows to skip
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Skip first 20 rows
    /// query.offset(20)
    ///
    /// // Pagination: page 3 with 10 items per page
    /// query.limit(10).offset(20)  // Skip 2 pages = 20 items
    /// ```
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Sets the maximum number of records to return.
    ///
    /// Limits the number of rows returned by the query. Essential for pagination
    /// and preventing accidentally fetching large result sets.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of rows to return
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Return at most 10 rows
    /// query.limit(10)
    ///
    /// // Pagination: 50 items per page
    /// query.limit(50).offset(page * 50)
    /// ```
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    // ========================================================================
    // Insert Operation
    // ========================================================================

    /// Inserts a new record into the database based on the model instance.
    ///
    /// This method serializes the model into a SQL INSERT statement with proper
    /// type handling for primitives, dates, UUIDs, and other supported types.
    ///
    /// # Type Binding Strategy
    ///
    /// The method uses string parsing as a temporary solution for type binding.
    /// Values are converted to strings via the model's `to_map()` method, then
    /// parsed back to their original types for proper SQL binding.
    ///
    /// # Supported Types for Insert
    ///
    /// - **Integers**: `i32`, `i64` (INTEGER, BIGINT)
    /// - **Boolean**: `bool` (BOOLEAN)
    /// - **Float**: `f64` (DOUBLE PRECISION)
    /// - **Text**: `String` (TEXT, VARCHAR)
    /// - **UUID**: `Uuid` (UUID) - All versions 1-7 supported
    /// - **DateTime**: `DateTime<Utc>` (TIMESTAMPTZ)
    /// - **NaiveDateTime**: (TIMESTAMP)
    /// - **NaiveDate**: (DATE)
    /// - **NaiveTime**: (TIME)
    ///
    /// # Arguments
    ///
    /// * `model` - Reference to the model instance to insert
    ///
    /// # Returns
    ///
    /// * `Ok(&Self)` - Reference to self for method chaining
    /// * `Err(sqlx::Error)` - Database error during insertion
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use uuid::Uuid;
    /// use chrono::Utc;
    ///
    /// let new_user = User {
    ///     id: Uuid::new_v4(),
    ///     username: "john_doe".to_string(),
    ///     email: "john@example.com".to_string(),
    ///     age: 25,
    ///     active: true,
    ///     created_at: Utc::now(),
    /// };
    ///
    /// db.model::<User>().insert(&new_user).await?;
    /// ```
    pub async fn insert(&mut self, model: &T) -> Result<&Self, sqlx::Error> {
        // Serialize model to a HashMap of column_name -> string_value
        let data_map = model.to_map();

        // Early return if no data to insert
        if data_map.is_empty() {
            return Ok(self);
        }

        let table_name = self.table_name.to_snake_case();
        let columns_info = T::columns();

        let mut target_columns = Vec::new();
        let mut bindings: Vec<(String, &str)> = Vec::new();

        // Build column list and collect values with their SQL types
        for (col_name, value) in data_map {
            // Strip the "r#" prefix if present (for Rust keywords used as field names)
            let col_name_clean = col_name.strip_prefix("r#").unwrap_or(&col_name).to_snake_case();
            target_columns.push(format!("\"{}\"", col_name_clean));

            // Find the SQL type for this column
            let sql_type = columns_info.iter().find(|c| c.name == col_name).map(|c| c.sql_type).unwrap_or("TEXT");

            bindings.push((value, sql_type));
        }

        // Generate placeholders with proper type casting for PostgreSQL
        let placeholders: Vec<String> = bindings
            .iter()
            .enumerate()
            .map(|(i, (_, sql_type))| match self.tx.driver() {
                Drivers::Postgres => {
                    let idx = i + 1;
                    // PostgreSQL requires explicit type casting for some types
                    if temporal::is_temporal_type(sql_type) {
                        // Use temporal module for type casting
                        format!("${}{}", idx, temporal::get_postgres_type_cast(sql_type))
                    } else {
                        match *sql_type {
                            "UUID" => format!("${}::UUID", idx),
                            _ => format!("${}", idx),
                        }
                    }
                }
                // MySQL and SQLite use simple ? placeholders
                _ => "?".to_string(),
            })
            .collect();

        // Construct the INSERT query
        let query_str = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            table_name,
            target_columns.join(", "),
            placeholders.join(", ")
        );

        // Debug: Uncomment to see generated SQL
        // println!("{}", query_str);

        let mut query = sqlx::query::<sqlx::Any>(&query_str);

        // Bind values using the optimized value_binding module
        // This provides type-safe binding with driver-specific optimizations
        for (val_str, sql_type) in bindings {
            // Create temporary AnyArguments to collect the bound value
            let mut temp_args = AnyArguments::default();

            // Use the ValueBinder trait for type-safe binding
            if temp_args.bind_value(&val_str, sql_type, &self.tx.driver()).is_ok() {
                // For now, we need to convert back to individual bindings
                // This is a workaround until we can better integrate AnyArguments
                match sql_type {
                    "INTEGER" | "INT" | "SERIAL" | "serial" | "int4" => {
                        if let Ok(val) = val_str.parse::<i32>() {
                            query = query.bind(val);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "BIGINT" | "INT8" | "int8" | "BIGSERIAL" => {
                        if let Ok(val) = val_str.parse::<i64>() {
                            query = query.bind(val);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "BOOLEAN" | "BOOL" | "bool" => {
                        if let Ok(val) = val_str.parse::<bool>() {
                            query = query.bind(val);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "DOUBLE PRECISION" | "FLOAT" | "float8" | "REAL" | "NUMERIC" | "DECIMAL" => {
                        if let Ok(val) = val_str.parse::<f64>() {
                            query = query.bind(val);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "UUID" => {
                        if let Ok(val) = val_str.parse::<Uuid>() {
                            query = query.bind(val.hyphenated().to_string());
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "TIMESTAMPTZ" | "DateTime" => {
                        if let Ok(val) = temporal::parse_datetime_utc(&val_str) {
                            let formatted = temporal::format_datetime_for_driver(&val, &self.tx.driver());
                            query = query.bind(formatted);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "TIMESTAMP" | "NaiveDateTime" => {
                        if let Ok(val) = temporal::parse_naive_datetime(&val_str) {
                            let formatted = temporal::format_naive_datetime_for_driver(&val, &self.tx.driver());
                            query = query.bind(formatted);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "DATE" | "NaiveDate" => {
                        if let Ok(val) = temporal::parse_naive_date(&val_str) {
                            let formatted = val.format("%Y-%m-%d").to_string();
                            query = query.bind(formatted);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    "TIME" | "NaiveTime" => {
                        if let Ok(val) = temporal::parse_naive_time(&val_str) {
                            let formatted = val.format("%H:%M:%S%.6f").to_string();
                            query = query.bind(formatted);
                        } else {
                            query = query.bind(val_str);
                        }
                    }
                    _ => {
                        query = query.bind(val_str);
                    }
                }
            } else {
                // Fallback: bind as string if type conversion fails
                query = query.bind(val_str);
            }
        }

        // Execute the INSERT query
        query.execute(self.tx.executor()).await?;
        Ok(self)
    }

    // ========================================================================
    // Query Execution Methods
    // ========================================================================

    /// Returns the generated SQL string for debugging purposes.
    ///
    /// This method constructs the SQL query string without executing it.
    /// Useful for debugging and logging query construction. Note that this
    /// shows placeholders (?, $1, etc.) rather than actual bound values.
    ///
    /// # Returns
    ///
    /// A `String` containing the SQL query that would be executed
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query = db.model::<User>()
    ///     .filter("age", ">=", 18)
    ///     .order("created_at DESC")
    ///     .limit(10);
    ///
    /// println!("SQL: {}", query.to_sql());
    /// // Output: SELECT * FROM "user" WHERE 1=1 AND "age" >= $1 ORDER BY created_at DESC
    /// ```
    pub fn to_sql(&self) -> String {
        let mut query = String::from("SELECT ");

        // Handle column selection
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }

        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");

        // Apply WHERE clauses with dummy arguments
        let mut dummy_args = AnyArguments::default();
        let mut dummy_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut dummy_args, &self.tx.driver(), &mut dummy_counter);
        }

        // Apply ORDER BY if present
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", &self.order_clauses.join(", ")));
        }

        query
    }

    /// Generates the list of column selection SQL arguments.
    ///
    /// This helper function constructs the column list for the SELECT statement.
    /// It handles:
    /// 1. Mapping specific columns if `select_columns` is set.
    /// 2. Defaulting to all columns from the struct `R` if no columns are specified.
    /// 3. applying `to_json(...)` casting for temporal types when using `AnyImpl` structs,
    ///    ensuring compatibility with the `FromAnyRow` deserialization logic.
    fn select_args_sql<R: AnyImpl>(&self) -> Vec<String> {
        let struct_cols = R::columns();

        if !struct_cols.is_empty() {
            if !self.select_columns.is_empty() {
                let mut args = Vec::new();
                for col_info in struct_cols {
                    let col_snake = col_info.column.to_snake_case();
                    let sql_type = col_info.sql_type;
                    if self.select_columns.contains(&col_snake) {
                        if is_temporal_type(sql_type) {
                            if !self.joins_clauses.is_empty() {
                                args.push(format!(
                                    "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                    self.table_name.to_snake_case(),
                                    col_snake,
                                    col_snake
                                ));
                            } else {
                                args.push(format!("to_json(\"{}\") #>> '{{}}' AS \"{}\"", col_snake, col_snake));
                            }
                        } else {
                            if !self.joins_clauses.is_empty() {
                                args.push(format!("\"{}\".\"{}\"", self.table_name.to_snake_case(), col_snake));
                            } else {
                                args.push(format!("\"{}\"", col_snake));
                            }
                        }
                    }
                }
                return args;
            } else {
                return struct_cols
                    .iter()
                    .map(|c| {
                        let col_snake = c.column.to_snake_case();
                        if is_temporal_type(c.sql_type) {
                            if !self.joins_clauses.is_empty() {
                                format!(
                                    "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                    self.table_name.to_snake_case(),
                                    col_snake,
                                    col_snake
                                )
                            } else {
                                format!("to_json(\"{}\") #>> '{{}}' AS \"{}\"", col_snake, col_snake)
                            }
                        } else {
                            if !self.joins_clauses.is_empty() {
                                format!("\"{}\".\"{}\"", self.table_name.to_snake_case(), col_snake)
                            } else {
                                format!("\"{}\"", col_snake)
                            }
                        }
                    })
                    .collect();
            }
        }

        if !self.select_columns.is_empty() {
            return self
                .select_columns
                .iter()
                .map(|c| if c.contains('(') { c.clone() } else { format!("\"{}\"", c) })
                .collect();
        }

        vec!["*".to_string()]
    }

    /// Executes the query and returns a list of results.
    ///
    /// This method builds and executes a SELECT query with all accumulated filters,
    /// ordering, and pagination settings. It returns all matching rows as a vector.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The result type. Must implement `FromRow` for deserialization from database rows.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<R>)` - Vector of results (empty if no matches)
    /// * `Err(sqlx::Error)` - Database error during query execution
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get all adult users, ordered by age, limited to 10
    /// let users: Vec<User> = db.model::<User>()
    ///     .filter("age", ">=", 18)
    ///     .order("age DESC")
    ///     .limit(10)
    ///     .scan()
    ///     .await?;
    ///
    /// // Get users by UUID
    /// let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")?;
    /// let users: Vec<User> = db.model::<User>()
    ///     .filter("id", "=", user_id)
    ///     .scan()
    ///     .await?;
    ///
    /// // Empty result is Ok
    /// let results: Vec<User> = db.model::<User>()
    ///     .filter("age", ">", 200)
    ///     .scan()
    ///     .await?;  // Returns empty Vec, not an error
    /// ```
    pub async fn scan<R>(mut self) -> Result<Vec<R>, sqlx::Error>
    where
        R: for<'r> FromRow<'r, AnyRow> + AnyImpl + Send + Unpin,
    {
        // Build SELECT clause
        let mut query = String::from("SELECT ");
        query.push_str(&self.select_args_sql::<R>().join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.tx.driver(), &mut arg_counter);
        }

        // Apply ORDER BY clauses
        // We join multiple clauses with commas to form a valid SQL ORDER BY statement
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        }

        // Apply LIMIT clause
        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ");
            match self.tx.driver() {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    arg_counter += 1;
                }
                _ => query.push('?'),
            }
            let _ = args.add(limit as i64);
        }

        // Apply OFFSET clause
        if let Some(offset) = self.offset {
            query.push_str(" OFFSET ");
            match self.tx.driver() {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    // arg_counter += 1; // Not needed as this is the last clause
                }
                _ => query.push('?'),
            }
            let _ = args.add(offset as i64);
        }

        // Execute query and fetch all results
        sqlx::query_as_with::<_, R, _>(&query, args).fetch_all(self.tx.executor()).await
    }

    /// Executes the query and returns only the first result.
    ///
    /// This method automatically adds `LIMIT 1` and orders by the Primary Key
    /// (if available) to ensure consistent results. It's optimized for fetching
    /// a single row and will return an error if no rows match.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The result type. Must implement `FromRow` for deserialization.
    ///
    /// # Returns
    ///
    /// * `Ok(R)` - The first matching row
    /// * `Err(sqlx::Error)` - No rows found or database error
    ///
    /// # Error Handling
    ///
    /// Returns `sqlx::Error::RowNotFound` if no rows match the query.
    /// Use `scan()` instead if you want an empty Vec rather than an error.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get a specific user by ID
    /// let user: User = db.model::<User>()
    ///     .filter("id", "=", 1)
    ///     .first()
    ///     .await?;
    ///
    /// // Get user by UUID
    /// let user_id = Uuid::new_v4();
    /// let user: User = db.model::<User>()
    ///     .filter("id", "=", user_id)
    ///     .first()
    ///     .await?;
    ///
    /// // Get the oldest user
    /// let oldest: User = db.model::<User>()
    ///     .order("age DESC")
    ///     .first()
    ///     .await?;
    ///
    /// // Error handling
    /// match db.model::<User>().filter("id", "=", 999).first().await {
    ///     Ok(user) => println!("Found: {:?}", user),
    ///     Err(sqlx::Error::RowNotFound) => println!("User not found"),
    ///     Err(e) => println!("Database error: {}", e),
    /// }
    /// ```
    pub async fn first<R>(mut self) -> Result<R, sqlx::Error>
    where
        R: for<'r> FromRow<'r, AnyRow> + AnyImpl + Send + Unpin,
    {
        // Build SELECT clause
        let mut query = String::from("SELECT ");
        query.push_str(&self.select_args_sql::<R>().join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.tx.driver(), &mut arg_counter);
        }

        // Find primary key column for consistent ordering
        let pk_column = T::columns()
            .iter()
            .find(|c| c.is_primary_key)
            .map(|c| c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case());

        // Apply ORDER BY clauses
        // We join multiple clauses with commas to form a valid SQL ORDER BY statement
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        } else if let Some(pk) = pk_column {
            // Fallback to PK ordering if no custom order is specified (ensures deterministic results)
            query.push_str(" ORDER BY ");
            query.push_str(&format!("\"{}\".\"{}\"", self.table_name.to_snake_case(), pk));
            query.push_str(" ASC");
        }

        // Always add LIMIT 1 for first() queries
        query.push_str(" LIMIT 1");

        // Execute query and fetch exactly one result
        sqlx::query_as_with::<_, R, _>(&query, args).fetch_one(self.tx.executor()).await
    }

    /// Executes the query and returns a single scalar value.
    ///
    /// This method is useful for fetching single values like counts, max/min values,
    /// or specific columns without mapping to a struct or tuple.
    ///
    /// # Type Parameters
    ///
    /// * `O` - The output type. Must implement `Decode` and `Type`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get count of users
    /// let count: i64 = db.model::<User>()
    ///     .select("count(*)")
    ///     .scalar()
    ///     .await?;
    ///
    /// // Get specific field
    /// let username: String = db.model::<User>()
    ///     .filter("id", "=", 1)
    ///     .select("username")
    ///     .scalar()
    ///     .await?;
    /// ```
    pub async fn scalar<O>(mut self) -> Result<O, sqlx::Error>
    where
        O: for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        // Build SELECT clause
        let mut query = String::from("SELECT ");

        if self.select_columns.is_empty() {
            return Err(sqlx::Error::ColumnNotFound("is not possible get data without column".to_string()));
        }

        let mut select_cols = Vec::with_capacity(self.select_columns.capacity());
        for col in self.select_columns {
            if !self.joins_clauses.is_empty() {
                if let Some((table, column)) = col.split_once(".") {
                    select_cols.push(format!("\"{}\".\"{}\"", table, column));
                } else {
                    select_cols.push(format!("\"{}\".\"{}\"", self.table_name.to_snake_case(), col));
                }
                continue;
            }
            select_cols.push(col);
        }

        query.push_str(&select_cols.join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");

        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.tx.driver(), &mut arg_counter);
        }

        // Apply ORDER BY
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", &self.order_clauses.join(", ")));
        }

        // Always add LIMIT 1 for scalar queries
        query.push_str(" LIMIT 1");

        // Execute query and fetch one row
        let row = sqlx::query_with::<_, _>(&query, args).fetch_one(self.tx.executor()).await?;

        // Get the first column
        row.try_get::<O, _>(0)
    }

    /// Updates a single column in the database.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to update
    /// * `value` - The new value
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    pub async fn update<V>(&mut self, col: &str, value: V) -> Result<u64, sqlx::Error>
    where
        V: ToString + Send + Sync,
    {
        let mut map = std::collections::HashMap::new();
        map.insert(col.to_string(), value.to_string());
        self.execute_update(map).await
    }

    /// Updates all columns based on the model instance.
    ///
    /// This method updates all active columns of the table with values from the provided model.
    ///
    /// # Arguments
    ///
    /// * `model` - The model instance containing new values
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    pub async fn updates(&mut self, model: &T) -> Result<u64, sqlx::Error> {
        self.execute_update(model.to_map()).await
    }

    /// Updates columns based on a partial model (struct implementing AnyImpl).
    ///
    /// This allows updating a subset of columns using a custom struct.
    /// The struct must implement `AnyImpl` (usually via `#[derive(FromAnyRow)]`).
    ///
    /// # Arguments
    ///
    /// * `partial` - The partial model containing new values
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    pub async fn update_partial<P: AnyImpl>(&mut self, partial: &P) -> Result<u64, sqlx::Error> {
        self.execute_update(partial.to_map()).await
    }

    /// Internal helper to execute an UPDATE query from a map of values.
    async fn execute_update(
        &mut self,
        data_map: std::collections::HashMap<String, String>,
    ) -> Result<u64, sqlx::Error> {
        let table_name = self.table_name.to_snake_case();
        let mut query = format!("UPDATE \"{}\" SET ", table_name);

        let mut bindings: Vec<(String, &str)> = Vec::new();
        let mut set_clauses = Vec::new();

        // Maintain argument counter for PostgreSQL ($1, $2, ...)
        let mut arg_counter = 1;

        // Build SET clause
        for (col_name, value) in data_map {
            // Strip the "r#" prefix if present
            let col_name_clean = col_name.strip_prefix("r#").unwrap_or(&col_name).to_snake_case();

            // Find the SQL type for this column from the Model metadata
            let sql_type = self
                .columns_info
                .iter()
                .find(|c| c.name == col_name || c.name == col_name_clean)
                .map(|c| c.sql_type)
                .unwrap_or("TEXT");

            // Generate placeholder
            let placeholder = match self.tx.driver() {
                Drivers::Postgres => {
                    let idx = arg_counter;
                    arg_counter += 1;

                    if temporal::is_temporal_type(sql_type) {
                        format!("${}{}", idx, temporal::get_postgres_type_cast(sql_type))
                    } else {
                        match sql_type {
                            "UUID" => format!("${}::UUID", idx),
                            _ => format!("${}", idx),
                        }
                    }
                }
                _ => "?".to_string(),
            };

            set_clauses.push(format!("\"{}\" = {}", col_name_clean, placeholder));
            bindings.push((value, sql_type));
        }

        // If no fields to update, return 0
        if set_clauses.is_empty() {
            return Ok(0);
        }

        query.push_str(&set_clauses.join(", "));

        // Build WHERE clause
        query.push_str(" WHERE 1=1");

        let mut args = AnyArguments::default();

        // Bind SET values
        for (val_str, sql_type) in bindings {
            if args.bind_value(&val_str, sql_type, &self.tx.driver()).is_err() {
                let _ = args.add(val_str);
            }
        }

        // Apply WHERE clauses (appending to args and query)
        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.tx.driver(), &mut arg_counter);
        }

        // Execute the UPDATE query
        let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;

        Ok(result.rows_affected())
    }

    /// Executes a DELETE query based on the current filters.
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows deleted
    /// * `Err(sqlx::Error)` - Database error
    pub async fn delete(mut self) -> Result<u64, sqlx::Error> {
        let mut query = String::from("DELETE FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");

        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.tx.driver(), &mut arg_counter);
        }

        let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;
        Ok(result.rows_affected())
    }
}
