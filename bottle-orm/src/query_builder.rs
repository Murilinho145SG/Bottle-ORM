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

use futures::future::BoxFuture;
use heck::ToSnakeCase;
use sqlx::{Any, Arguments, Decode, Encode, Row, Type, any::AnyArguments};
use std::marker::PhantomData;
use uuid::Uuid;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{
    AnyImpl, Error,
    any_struct::FromAnyRow,
    database::{Connection, Drivers},
    model::{ColumnInfo, Model},
    temporal::{self, is_temporal_type},
    value_binding::ValueBinder,
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
/// });\n/// ```
pub type FilterFn = Box<dyn Fn(&mut String, &mut AnyArguments<'_>, &Drivers, &mut usize) + Send + Sync>;

// ============================================================================
// Comparison Operators Enum
// ============================================================================

/// Type-safe comparison operators for filter conditions.
///
/// Use these instead of string operators for autocomplete support and type safety.
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::Op;
///
/// db.model::<User>()
///     .filter(user_fields::AGE, Op::Gte, 18)
///     .filter(user_fields::NAME, Op::Like, "%John%")
///     .scan()
///     .await?;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Equal: `=`
    Eq,
    /// Not Equal: `!=` or `<>`
    Ne,
    /// Greater Than: `>`
    Gt,
    /// Greater Than or Equal: `>=`
    Gte,
    /// Less Than: `<`
    Lt,
    /// Less Than or Equal: `<=`
    Lte,
    /// SQL LIKE pattern matching
    Like,
    /// SQL NOT LIKE pattern matching
    NotLike,
    /// SQL IN (for arrays/lists)
    In,
    /// SQL NOT IN
    NotIn,
}

impl Op {
    /// Converts the operator to its SQL string representation.
    pub fn as_sql(&self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Gt => ">",
            Op::Gte => ">=",
            Op::Lt => "<",
            Op::Lte => "<=",
            Op::Like => "LIKE",
            Op::NotLike => "NOT LIKE",
            Op::In => "IN",
            Op::NotIn => "NOT IN",
        }
    }
}

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
pub struct QueryBuilder<'a, T, E> {
    /// Reference to the database connection pool
    pub(crate) tx: E,

    /// Database driver type
    pub(crate) driver: Drivers,

    /// Name of the database table (in original case)
    pub(crate) table_name: &'static str,

    pub(crate) alias: Option<String>,

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

    /// Map of table names to their aliases used in JOINS
    pub(crate) join_aliases: std::collections::HashMap<String, String>,

    /// Maximum number of rows to return (LIMIT)
    pub(crate) limit: Option<usize>,

    /// Number of rows to skip (OFFSET)
    pub(crate) offset: Option<usize>,

    /// Activate debug mode in query
    pub(crate) debug_mode: bool,

    /// Clauses for GROUP BY
    pub(crate) group_by_clauses: Vec<String>,

    /// Clauses for HAVING
    pub(crate) having_clauses: Vec<FilterFn>,

    /// Distinct flag
    pub(crate) is_distinct: bool,

    /// Columns to omit from the query results (inverse of select_columns)
    pub(crate) omit_columns: Vec<String>,

    /// Whether to include soft-deleted records in query results
    pub(crate) with_deleted: bool,

    /// PhantomData to bind the generic type T
    pub(crate) _marker: PhantomData<&'a T>,
}

// ============================================================================
// QueryBuilder Implementation
// ============================================================================

impl<'a, T, E> QueryBuilder<'a, T, E>
where
    T: Model + Send + Sync + Unpin,
    E: Connection + Send,
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
    pub fn new(
        tx: E,
        driver: Drivers,
        table_name: &'static str,
        columns_info: Vec<ColumnInfo>,
        columns: Vec<String>,
    ) -> Self {
        // Pre-populate omit_columns with globally omitted columns (from #[orm(omit)] attribute)
        let omit_columns: Vec<String> =
            columns_info.iter().filter(|c| c.omit).map(|c| c.name.to_snake_case()).collect();

        Self {
            tx,
            alias: None,
            driver,
            table_name,
            columns_info,
            columns,
            debug_mode: false,
            select_columns: Vec::new(),
            where_clauses: Vec::new(),
            order_clauses: Vec::new(),
            joins_clauses: Vec::new(),
            join_aliases: std::collections::HashMap::new(),
            group_by_clauses: Vec::new(),
            having_clauses: Vec::new(),
            is_distinct: false,
            omit_columns,
            limit: None,
            offset: None,
            with_deleted: false,
            _marker: PhantomData,
        }
    }

    /// Returns the table name or alias if set.
    pub(crate) fn get_table_identifier(&self) -> String {
        self.alias.clone().unwrap_or_else(|| self.table_name.to_snake_case())
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
    ///     .filter("age", Op::Gte, 18)
    ///     .filter("active", Op::Eq, true)
    ///     .filter("role", Op::Eq, "admin")
    /// ```
    pub fn filter<V>(mut self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let op_str = op.as_sql();
        let table_id = self.get_table_identifier();
        // Check if the column exists in the main table to avoid ambiguous references in JOINS
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col.split_once(".") {
                // If explicit table prefix is provided, use it
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                // If it's a known column of the main table, apply the table name/alias prefix
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                // Otherwise leave it unqualified so the DB can resolve it (or fail if ambiguous)
                query.push_str(&format!("\"{}\"", col));
            }
            query.push(' ');
            query.push_str(op_str);
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
    /// // Equivalent to filter("age", Op::Eq, 18)
    /// query.equals("age", 18)
    /// ```
    pub fn equals<V>(self, col: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter(col, Op::Eq, value)
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

    /// Defines a SQL alias for the primary table in the query.
    ///
    /// This method allows you to set a short alias for the model's underlying table.
    /// It is highly recommended when writing complex queries with multiple `JOIN` clauses,
    /// preventing the need to repeat the full table name in `.filter()`, `.equals()`, or `.select()`.
    ///
    /// # Arguments
    ///
    /// * `alias` - A string slice representing the alias to be used (e.g., "u", "rp").
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Using 'u' as an alias for the User table
    /// let results = db.model::<User>()
    ///     .alias("u")
    ///     .join("role_permissions rp", "rp.role_id = u.role")
    ///     .equals("u.id", user_id)
    ///     .select("u.username, rp.permission_id")
    ///     .scan_as::<UserPermissionDTO>()
    ///     .await?;
    /// ```
    pub fn alias(mut self, alias: &str) -> Self {
        self.alias = Some(alias.to_string());
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
    // pub fn preload(self) -> Self {
    //     // TODO: Implement relationship preloading
    //     self
    // }

    /// Activates debug mode for this query.
    ///
    /// When enabled, the generated SQL query will be logged using the `log` crate
    /// at the `DEBUG` level before execution.
    ///
    /// # Note
    ///
    /// To see the output, you must initialize a logger in your application (e.g., using `env_logger`)
    /// and configure it to display `debug` logs for `bottle_orm`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("active", "=", true)
    ///     .debug() // Logs SQL: SELECT * FROM "user" WHERE "active" = $1
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn debug(mut self) -> Self {
        self.debug_mode = true;
        self
    }

    /// Adds an IS NULL filter for the specified column.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to check for NULL
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .is_null("deleted_at")
    ///     .scan()
    ///     .await?;
    /// // SQL: SELECT * FROM "user" WHERE "deleted_at" IS NULL
    /// ```
    pub fn is_null(mut self, col: &str) -> Self {
        let col_owned = col.to_string();
        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col_owned.to_snake_case());
        let clause: FilterFn = Box::new(move |query, _args, _driver, _arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col_owned.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col_owned));
            } else {
                query.push_str(&format!("\"{}\"", col_owned));
            }
            query.push_str(" IS NULL");
        });
        self.where_clauses.push(clause);
        self
    }

    /// Adds an IS NOT NULL filter for the specified column.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to check for NOT NULL
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .is_not_null("email")
    ///     .scan()
    ///     .await?;
    /// // SQL: SELECT * FROM "user" WHERE "email" IS NOT NULL
    /// ```
    pub fn is_not_null(mut self, col: &str) -> Self {
        let col_owned = col.to_string();
        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col_owned.to_snake_case());
        let clause: FilterFn = Box::new(move |query, _args, _driver, _arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col_owned.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col_owned));
            } else {
                query.push_str(&format!("\"{}\"", col_owned));
            }
            query.push_str(" IS NOT NULL");
        });
        self.where_clauses.push(clause);
        self
    }

    /// Includes soft-deleted records in query results.
    ///
    /// By default, queries on models with a `#[orm(soft_delete)]` column exclude
    /// records where that column is not NULL. This method disables that filter.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get all users including deleted ones
    /// db.model::<User>()
    ///     .with_deleted()
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn with_deleted(mut self) -> Self {
        self.with_deleted = true;
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
        let trimmed_value = s_query.replace(" ", "");
        let values = trimmed_value.split_once("=");
        let parsed_query: String;
        if let Some((first, second)) = values {
            let ref_table = first.split_once(".").expect("failed to parse JOIN clause");
            let to_table = second.split_once(".").expect("failed to parse JOIN clause");
            parsed_query = format!("\"{}\".\"{}\" = \"{}\".\"{}\"", ref_table.0, ref_table.1, to_table.0, to_table.1);
        } else {
            panic!("Failed to parse JOIN, Ex to use: .join(\"table2\", \"table2.column = table.column\")")
        }

        if let Some((table_name, alias)) = table.split_once(" ") {
            self.join_aliases.insert(table_name.to_snake_case(), alias.to_string());
            self.joins_clauses.push(format!("JOIN \"{}\" {} ON {}", table_name, alias, parsed_query));
        } else {
            self.joins_clauses.push(format!("JOIN \"{}\" ON {}", table, parsed_query));
        }
        self
    }

    /// Internal helper for specific join types
    fn join_generic(mut self, join_type: &str, table: &str, s_query: &str) -> Self {
        let trimmed_value = s_query.replace(" ", "");
        let values = trimmed_value.split_once("=");
        let parsed_query: String;
        if let Some((first, second)) = values {
            let ref_table = first.split_once(".").expect("failed to parse JOIN clause");
            let to_table = second.split_once(".").expect("failed to parse JOIN clause");
            parsed_query = format!("\"{}\".\"{}\" = \"{}\".\"{}\"", ref_table.0, ref_table.1, to_table.0, to_table.1);
        } else {
            panic!("Failed to parse JOIN, Ex to use: .join(\"table2\", \"table2.column = table.column\")")
        }

        if let Some((table_name, alias)) = table.split_once(" ") {
            self.join_aliases.insert(table_name.to_snake_case(), alias.to_string());
            self.joins_clauses.push(format!("{} JOIN \"{}\" {} ON {}", join_type, table_name, alias, parsed_query));
        } else {
            self.joins_clauses.push(format!("{} JOIN \"{}\" ON {}", join_type, table, parsed_query));
        }
        self
    }

    /// Adds a LEFT JOIN clause.
    ///
    /// Performs a LEFT JOIN with another table. Returns all records from the left table,
    /// and the matched records from the right table (or NULL if no match).
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition (e.g., "users.id = posts.user_id")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get all users and their posts (if any)
    /// let users_with_posts = db.model::<User>()
    ///     .left_join("posts", "users.id = posts.user_id")
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn left_join(self, table: &str, on: &str) -> Self {
        self.join_generic("LEFT", table, on)
    }

    /// Adds a RIGHT JOIN clause.
    ///
    /// Performs a RIGHT JOIN with another table. Returns all records from the right table,
    /// and the matched records from the left table (or NULL if no match).
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let posts_with_users = db.model::<Post>()
    ///     .right_join("users", "posts.user_id = users.id")
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn right_join(self, table: &str, on: &str) -> Self {
        self.join_generic("RIGHT", table, on)
    }

    /// Adds an INNER JOIN clause.
    ///
    /// Performs an INNER JOIN with another table. Returns records that have matching
    /// values in both tables.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get only users who have posts
    /// let active_users = db.model::<User>()
    ///     .inner_join("posts", "users.id = posts.user_id")
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn inner_join(self, table: &str, on: &str) -> Self {
        self.join_generic("INNER", table, on)
    }

    /// Adds a FULL JOIN clause.
    ///
    /// Performs a FULL OUTER JOIN. Returns all records when there is a match in
    /// either left or right table.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition
    ///
    /// # Note
    ///
    /// Support for FULL JOIN depends on the underlying database engine (e.g., SQLite
    /// does not support FULL JOIN directly).
    pub fn full_join(self, table: &str, on: &str) -> Self {
        self.join_generic("FULL", table, on)
    }

    /// Marks the query to return DISTINCT results.
    ///
    /// Adds the `DISTINCT` keyword to the SELECT statement, ensuring that unique
    /// rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get unique ages of users
    /// let unique_ages: Vec<i32> = db.model::<User>()
    ///     .select("age")
    ///     .distinct()
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn distinct(mut self) -> Self {
        self.is_distinct = true;
        self
    }

    /// Adds a GROUP BY clause to the query.
    ///
    /// Groups rows that have the same values into summary rows. Often used with
    /// aggregate functions (COUNT, MAX, MIN, SUM, AVG).
    ///
    /// # Arguments
    ///
    /// * `columns` - Comma-separated list of columns to group by
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Count users by age group
    /// let stats: Vec<(i32, i64)> = db.model::<User>()
    ///     .select("age, COUNT(*)")
    ///     .group_by("age")
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn group_by(mut self, columns: &str) -> Self {
        self.group_by_clauses.push(columns.to_string());
        self
    }

    /// Adds a HAVING clause to the query.
    ///
    /// Used to filter groups created by `group_by`. Similar to `filter` (WHERE),
    /// but operates on grouped records and aggregate functions.
    ///
    /// # Arguments
    ///
    /// * `col` - The column or aggregate function to filter on
    /// * `op` - Comparison operator
    /// * `value` - Value to compare against
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get ages with more than 5 users
    /// let popular_ages = db.model::<User>()
    ///     .select("age, COUNT(*)")
    ///     .group_by("age")
    ///     .having("COUNT(*)", ">", 5)
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn having<V>(mut self, col: &'static str, op: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            query.push_str(col);
            query.push(' ');
            query.push_str(op);
            query.push(' ');

            match driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    *arg_counter += 1;
                }
                _ => query.push('?'),
            }
            let _ = args.add(value.clone());
        });

        self.having_clauses.push(clause);
        self
    }

    /// Returns the COUNT of rows matching the query.
    ///
    /// A convenience method that automatically sets `SELECT COUNT(*)` and returns
    /// the result as an `i64`.
    ///
    /// # Returns
    ///
    /// * `Ok(i64)` - The count of rows
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user_count = db.model::<User>().count().await?;
    /// ```
    pub async fn count(mut self) -> Result<i64, sqlx::Error> {
        self.select_columns = vec!["COUNT(*)".to_string()];
        self.scalar::<i64>().await
    }

    /// Returns the SUM of the specified column.
    ///
    /// Calculates the sum of a numeric column.
    ///
    /// # Arguments
    ///
    /// * `column` - The column to sum
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let total_age: i64 = db.model::<User>().sum("age").await?;
    /// ```
    pub async fn sum<N>(mut self, column: &str) -> Result<N, sqlx::Error>
    where
        N: for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        self.select_columns = vec![format!("SUM({})", column)];
        self.scalar::<N>().await
    }

    /// Returns the AVG of the specified column.
    ///
    /// Calculates the average value of a numeric column.
    ///
    /// # Arguments
    ///
    /// * `column` - The column to average
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let avg_age: f64 = db.model::<User>().avg("age").await?;
    /// ```
    pub async fn avg<N>(mut self, column: &str) -> Result<N, sqlx::Error>
    where
        N: for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        self.select_columns = vec![format!("AVG({})", column)];
        self.scalar::<N>().await
    }

    /// Returns the MIN of the specified column.
    ///
    /// Finds the minimum value in a column.
    ///
    /// # Arguments
    ///
    /// * `column` - The column to check
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let min_age: i32 = db.model::<User>().min("age").await?;
    /// ```
    pub async fn min<N>(mut self, column: &str) -> Result<N, sqlx::Error>
    where
        N: for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        self.select_columns = vec![format!("MIN({})", column)];
        self.scalar::<N>().await
    }

    /// Returns the MAX of the specified column.
    ///
    /// Finds the maximum value in a column.
    ///
    /// # Arguments
    ///
    /// * `column` - The column to check
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let max_age: i32 = db.model::<User>().max("age").await?;
    /// ```
    pub async fn max<N>(mut self, column: &str) -> Result<N, sqlx::Error>
    where
        N: for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        self.select_columns = vec![format!("MAX({})", column)];
        self.scalar::<N>().await
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
    /// allows you to specify exactly which columns should be returned.
    ///
    /// **Note:** Columns are pushed exactly as provided, without automatic
    /// snake_case conversion, allowing for aliases and raw SQL fragments.
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
    /// // Select with SQL functions and aliases (now supported)
    /// query.select("COUNT(*) as total_count")
    /// ```
    pub fn select(mut self, columns: &str) -> Self {
        self.select_columns.push(columns.to_string());
        self
    }

    /// Excludes specific columns from the query results.
    ///
    /// This is the inverse of `select()`. Instead of specifying which columns to include,
    /// you specify which columns to exclude. All other columns will be returned.
    ///
    /// # Arguments
    ///
    /// * `columns` - Comma-separated list of column names to exclude
    ///
    /// # Priority
    ///
    /// If both `select()` and `omit()` are used, `select()` takes priority.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Exclude password from results
    /// let user = db.model::<User>()
    ///     .omit("password")
    ///     .first()
    ///     .await?;
    ///
    /// // Exclude multiple fields
    /// let user = db.model::<User>()
    ///     .omit("password, secret_token")
    ///     .first()
    ///     .await?;
    ///
    /// // Using with generated field constants (autocomplete support)
    /// let user = db.model::<User>()
    ///     .omit(user_fields::PASSWORD)
    ///     .first()
    ///     .await?;
    /// ```
    pub fn omit(mut self, columns: &str) -> Self {
        for col in columns.split(',') {
            self.omit_columns.push(col.trim().to_snake_case());
        }
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
    pub fn insert<'b>(&'b mut self, model: &'b T) -> BoxFuture<'b, Result<(), sqlx::Error>> {
        Box::pin(async move {
            // Serialize model to a HashMap of column_name -> string_value
            let data_map = model.to_map();

            // Early return if no data to insert
            if data_map.is_empty() {
                return Ok(());
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
                .map(|(i, (_, sql_type))| match self.driver {
                    Drivers::Postgres => {
                        let idx = i + 1;
                        // PostgreSQL requires explicit type casting for some types
                        if temporal::is_temporal_type(sql_type) {
                            // Use temporal module for type casting
                            format!("${}{}", idx, temporal::get_postgres_type_cast(sql_type))
                        } else {
                            match *sql_type {
                                "UUID" => format!("${}::UUID", idx),
                                "JSONB" | "jsonb" => format!("${}::JSONB", idx),
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

            // If debug mode is enabled, log the generated SQL query before execution
            if self.debug_mode {
                log::debug!("SQL: {}", query_str);
            }

            let mut query = sqlx::query::<sqlx::Any>(&query_str);

            // Bind values using the optimized value_binding module
            // This provides type-safe binding with driver-specific optimizations
            for (val_str, sql_type) in bindings {
                // Create temporary AnyArguments to collect the bound value
                let mut temp_args = AnyArguments::default();

                // Use the ValueBinder trait for type-safe binding
                if temp_args.bind_value(&val_str, sql_type, &self.driver).is_ok() {
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
                                let formatted = temporal::format_datetime_for_driver(&val, &self.driver);
                                query = query.bind(formatted);
                            } else {
                                query = query.bind(val_str);
                            }
                        }
                        "TIMESTAMP" | "NaiveDateTime" => {
                            if let Ok(val) = temporal::parse_naive_datetime(&val_str) {
                                let formatted = temporal::format_naive_datetime_for_driver(&val, &self.driver);
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
            Ok(())
        })
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

        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        // Handle column selection
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }

        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");

        if let Some(alias) = &self.alias {
            query.push_str(&format!("{} ", alias));
        }

        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses with dummy arguments
        let mut dummy_args = AnyArguments::default();
        let mut dummy_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut dummy_args, &self.driver, &mut dummy_counter);
        }

        // Apply GROUP BY
        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        // Apply HAVING
        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(&mut query, &mut dummy_args, &self.driver, &mut dummy_counter);
            }
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
        let table_id = self.get_table_identifier();

        if !struct_cols.is_empty() {
            if !self.select_columns.is_empty() {
                let mut args = Vec::new();

                // Flatten potential multi-column strings like "col1, col2"
                // This ensures each column is processed individually for prefixes and temporal types
                let mut flat_selects = Vec::new();
                for s in &self.select_columns {
                    if s.contains(',') {
                        for sub in s.split(',') {
                            flat_selects.push(sub.trim().to_string());
                        }
                    } else {
                        flat_selects.push(s.trim().to_string());
                    }
                }

                for col_info in struct_cols {
                    let col_snake = col_info.column.to_snake_case();
                    let sql_type = col_info.sql_type;

                    // Check if this column (or table.column) is in our select list
                    // We check against the column name alone OR the table-qualified name
                    let is_selected = flat_selects.iter().any(|s| {
                        if s == &col_snake {
                            return true;
                        }
                        if let Some((t, c)) = s.split_once('.') {
                            let t_clean = t.trim().trim_matches('"');
                            let c_clean = c.trim().trim_matches('"');
                            // Matches if the table prefix is either the original table name or the alias
                            return (t_clean == table_id || t_clean == self.table_name.to_snake_case())
                                && c_clean == col_snake;
                        }
                        false
                    });

                    if is_selected {
                        if is_temporal_type(sql_type) && matches!(self.driver, Drivers::Postgres) {
                            if !self.joins_clauses.is_empty() || self.alias.is_some() {
                                args.push(format!(
                                    "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                    table_id,
                                    col_snake,
                                    col_snake
                                ));
                            } else {
                                args.push(format!("to_json(\"{}\") #>> '{{}}' AS \"{}\"", col_snake, col_snake));
                            }
                        } else if !self.joins_clauses.is_empty() || self.alias.is_some() {
                            args.push(format!("\"{}\".\"{}\"", table_id, col_snake));
                        } else {
                            args.push(format!("\"{}\"", col_snake));
                        }
                    }
                }
                return args;
            } else {
                // For omitted columns, return 'omited' as placeholder value
                return struct_cols
                    .iter()
                    .map(|c| {
                        let col_snake = c.column.to_snake_case();
                        let is_omitted = self.omit_columns.contains(&col_snake);
                        
                        // table_to_alias is used for the result set mapping (AS "table__col")
                        // It MUST use the original table name snake_cased for the ORM to map it correctly
                        let table_to_alias = if !c.table.is_empty() {
                            c.table.to_snake_case()
                        } else {
                            self.table_name.to_snake_case()
                        };

                        // table_to_ref is used in the SELECT clause (SELECT "table"."col")
                        // It uses the alias if defined, or the original table name
                        let table_to_ref = if !c.table.is_empty() {
                            let c_table_snake = c.table.to_snake_case();
                            if c_table_snake == self.table_name.to_snake_case() {
                                table_id.clone()
                            } else {
                                // Check if we have an alias for this joined table
                                self.join_aliases.get(&c_table_snake).cloned().unwrap_or(c_table_snake)
                            }
                        } else {
                            table_id.clone()
                        };

                        if is_omitted {
                            // Return type-appropriate placeholder based on sql_type
                            let placeholder = match c.sql_type {
                                // String types
                                "TEXT" | "VARCHAR" | "CHAR" | "STRING" => "'omited'",
                                // Date/Time types - use epoch timestamp
                                "TIMESTAMP" | "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => "'1970-01-01T00:00:00Z'",
                                "DATE" => "'1970-01-01'",
                                "TIME" => "'00:00:00'",
                                // Numeric types
                                "INTEGER" | "INT" | "SMALLINT" | "BIGINT" | "INT4" | "INT8" => "0",
                                "REAL" | "FLOAT" | "DOUBLE" | "FLOAT4" | "FLOAT8" | "DECIMAL" | "NUMERIC" => "0.0",
                                // Boolean
                                "BOOLEAN" | "BOOL" => "false",
                                // UUID - nil UUID
                                "UUID" => "'00000000-0000-0000-0000-000000000000'",
                                // JSON types
                                "JSON" | "JSONB" => "'{}'",
                                // Default fallback for unknown types
                                _ => "'omited'",
                            };
                            format!("{} AS \"{}__{}\"", placeholder, table_to_alias, col_snake)
                        } else if is_temporal_type(c.sql_type) && matches!(self.driver, Drivers::Postgres) {
                            format!(
                                "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}__{}\"",
                                table_to_ref, col_snake, table_to_alias, col_snake
                            )
                        } else {
                            format!("\"{}\".\"{}\" AS \"{}__{}\"", table_to_ref, col_snake, table_to_alias, col_snake)
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
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        // Apply default soft delete filter if not disabled
        if !self.with_deleted {
            if let Some(soft_delete_col) = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name) {
                self = self.is_null(soft_delete_col);
            }
        }

        // Build SELECT clause
        let mut query = String::from("SELECT ");

        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        query.push_str(&self.select_args_sql::<R>().join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if let Some(alias) = &self.alias {
            query.push_str(&format!("{} ", alias));
        }

        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.driver, &mut arg_counter);
        }

        // Apply GROUP BY
        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        // Apply HAVING
        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }
        }

        // Apply ORDER BY clauses
        // We join multiple clauses with commas to form a valid SQL ORDER BY statement
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        }

        // Apply LIMIT clause
        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ");
            match self.driver {
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
            match self.driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    // arg_counter += 1; // Not needed as this is the last clause
                }
                _ => query.push('?'),
            }
            let _ = args.add(offset as i64);
        }

        // Print SQL query to logs if debug mode is active
        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        // Execute query and fetch all results
        let rows = sqlx::query_with(&query, args).fetch_all(self.tx.executor()).await?;

        rows.iter().map(|row| R::from_any_row(row)).collect()
    }

    /// Executes the query and maps the result to a custom DTO.
    ///
    /// Ideal for JOINs and projections where the return type is not a full Model.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The target result type. Must implement `FromAnyRow` and `AnyImpl`.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<R>)` - Vector of results mapped to type `R`.
    /// * `Err(sqlx::Error)` - Database error.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(FromAnyRow)]
    /// struct UserRoleDTO {
    ///     username: String,
    ///     role_name: String,
    /// }
    ///
    /// let results: Vec<UserRoleDTO> = db.model::<User>()
    ///     .inner_join("roles", "users.role_id = roles.id")
    ///     .select("users.username, roles.name as role_name")
    ///     .scan_as::<UserRoleDTO>()
    ///     .await?;
    /// ```
    pub async fn scan_as<R>(mut self) -> Result<Vec<R>, sqlx::Error>
    where
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        // Apply default soft delete filter if not disabled
        if !self.with_deleted {
            if let Some(soft_delete_col) = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name) {
                self = self.is_null(soft_delete_col);
            }
        }

        let mut query = String::from("SELECT ");
        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        let table_id = self.get_table_identifier();

        if self.select_columns.is_empty() {
            let mut select_args = Vec::new();
            let struct_cols = R::columns();
            let main_table_snake = self.table_name.to_snake_case();

            for c in struct_cols {
                let c_name = c.column.to_snake_case();

                // Determine if we should use the table name from AnyInfo
                // If it matches a joined table or the main table, we use it.
                // Otherwise (like UserDTO), we default to the main table.
                let mut table_to_use = table_id.clone();
                if !c.table.is_empty() {
                    let c_table_snake = c.table.to_snake_case();
                    if c_table_snake == main_table_snake
                        || self.joins_clauses.iter().any(|j| j.contains(&format!("JOIN \"{}\"", c_table_snake)))
                    {
                        if c_table_snake == main_table_snake {
                            table_to_use = table_id.clone();
                        } else {
                            // Use join alias if available
                            table_to_use = self.join_aliases.get(&c_table_snake).cloned().unwrap_or(c_table_snake);
                        }
                    }
                }

                if is_temporal_type(c.sql_type) && matches!(self.driver, Drivers::Postgres) {
                    select_args
                        .push(format!("to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"", table_to_use, c_name, c_name));
                } else {
                    select_args.push(format!("\"{}\".\"{}\" AS \"{}\"", table_to_use, c_name, c_name));
                }
            }

            if select_args.is_empty() {
                query.push('*');
            } else {
                query.push_str(&select_args.join(", "));
            }
        } else {
            let mut select_cols = Vec::with_capacity(self.select_columns.capacity());
            let struct_cols = R::columns();

            // Flatten multi-column strings
            let mut flat_selects = Vec::new();
            for s in &self.select_columns {
                if s.contains(',') {
                    for sub in s.split(',') {
                        flat_selects.push(sub.trim().to_string());
                    }
                } else {
                    flat_selects.push(s.trim().to_string());
                }
            }

            for col in &flat_selects {
                let col_trimmed = col.trim();
                if col_trimmed == "*" {
                    for c in &self.columns_info {
                        let c_name = c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case();
                        let mut is_c_temporal = false;
                        if let Some(r_info) = struct_cols.iter().find(|rc| rc.column.to_snake_case() == c_name) {
                            if is_temporal_type(r_info.sql_type) {
                                is_c_temporal = true;
                            }
                        }

                        if is_c_temporal && matches!(self.driver, Drivers::Postgres) {
                            select_cols.push(format!(
                                "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                table_id,
                                c_name,
                                c_name
                            ));
                        } else {
                            select_cols.push(format!(
                                "\"{}\".\"{}\" AS \"{}\"",
                                table_id,
                                c_name,
                                c_name
                            ));
                        }
                    }
                    continue;
                }

                // Check if this column is temporal in the target DTO
                let mut is_temporal = false;

                // We need to keep the lowercase string alive to use its slice in col_name
                let col_lower = col_trimmed.to_lowercase();
                let mut col_name = col_trimmed;

                // Handle aliases (e.g., "created_at as time" or "user.created_at as time")
                if let Some((_, alias)) = col_lower.split_once(" as ") {
                    col_name = alias.trim().trim_matches('"').trim_matches('\'');
                } else if col_trimmed.contains('.') {
                    if let Some((_, actual_col)) = col_trimmed.split_once('.') {
                        col_name = actual_col.trim().trim_matches('"').trim_matches('\'');
                    }
                }

                if let Some(info) = struct_cols.iter().find(|c| c.column.to_snake_case() == col_name.to_snake_case()) {
                    if is_temporal_type(info.sql_type) {
                        is_temporal = true;
                    }
                }

                if col_trimmed.contains('.') {
                    if let Some((table, column)) = col_trimmed.split_once('.') {
                        let clean_table = table.trim().trim_matches('"');
                        let clean_column = column.trim().trim_matches('"').split_whitespace().next().unwrap_or(column);

                        if clean_column == "*" {
                            let mut expanded = false;
                            let table_to_compare = clean_table.to_snake_case();
                            if table_to_compare == self.table_name.to_snake_case() || table_to_compare == table_id {
                                for c in &self.columns_info {
                                    let c_name = c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case();
                                    let mut is_c_temporal = false;
                                    if let Some(r_info) =
                                        struct_cols.iter().find(|rc| rc.column.to_snake_case() == c_name)
                                    {
                                        if is_temporal_type(r_info.sql_type) {
                                            is_c_temporal = true;
                                        }
                                    }

                                    if is_c_temporal && matches!(self.driver, Drivers::Postgres) {
                                        select_cols.push(format!(
                                            "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                            clean_table, c_name, c_name
                                        ));
                                    } else {
                                        select_cols
                                            .push(format!("\"{}\".\"{}\" AS \"{}\"", clean_table, c_name, c_name));
                                    }
                                }
                                expanded = true;
                            }

                            if !expanded {
                                select_cols.push(format!("\"{}\".*", clean_table));
                            }
                        } else if is_temporal && matches!(self.driver, Drivers::Postgres) {
                            select_cols.push(format!(
                                "to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"",
                                clean_table, clean_column, col_name
                            ));
                        } else {
                            select_cols.push(format!("\"{}\".\"{}\" AS \"{}\"", clean_table, clean_column, col_name));
                        }
                    }
                } else if is_temporal && matches!(self.driver, Drivers::Postgres) {
                    // Extract column name from potential expression
                    let clean_col = col_trimmed.trim_matches('"');
                    select_cols.push(format!("to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"", table_id, clean_col, col_name));
                } else if col_trimmed != col_name {
                    select_cols.push(format!("{} AS \"{}\"", col_trimmed, col_name));
                } else {
                    let is_main_col = self.columns.contains(&col_trimmed.to_snake_case());
                    if is_main_col {
                        select_cols.push(format!("\"{}\".\"{}\"", table_id, col_trimmed));
                    } else {
                        select_cols.push(format!("\"{}\"", col_trimmed));
                    }
                }
            }
            query.push_str(&select_cols.join(", "));
        }

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if let Some(alias) = &self.alias {
            query.push_str(&format!("{} ", alias));
        }

        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        let mut args = sqlx::any::AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.driver, &mut arg_counter);
        }

        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }
        }

        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        }

        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ");
            match self.driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    arg_counter += 1;
                }
                _ => query.push('?'),
            }
            let _ = args.add(limit as i64);
        }

        if let Some(offset) = self.offset {
            query.push_str(" OFFSET ");
            match self.driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                }
                _ => query.push('?'),
            }
            let _ = args.add(offset as i64);
        }

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let rows = sqlx::query_with(&query, args).fetch_all(self.tx.executor()).await?;
        rows.iter().map(|row| R::from_any_row(row)).collect()
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
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        // Apply default soft delete filter if not disabled
        if !self.with_deleted {
            if let Some(soft_delete_col) = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name) {
                self = self.is_null(soft_delete_col);
            }
        }

        // Build SELECT clause
        let mut query = String::from("SELECT ");

        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        query.push_str(&self.select_args_sql::<R>().join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if let Some(alias) = &self.alias {
            query.push_str(&format!("{} ", alias));
        }
        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.driver, &mut arg_counter);
        }

        // Apply GROUP BY
        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        // Apply HAVING
        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }
        }

        // Find primary key column for consistent ordering
        let pk_column = T::columns()
            .iter()
            .find(|c| c.is_primary_key)
            .map(|c| c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case());

        let table_id = self.get_table_identifier();

        // Apply ORDER BY clauses
        // We join multiple clauses with commas to form a valid SQL ORDER BY statement
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        } else if let Some(pk) = pk_column {
            // Fallback to PK ordering if no custom order is specified (ensures deterministic results)
            query.push_str(" ORDER BY ");
            query.push_str(&format!("\"{}\".\"{}\"", table_id, pk));
            query.push_str(" ASC");
        }

        // Always add LIMIT 1 for first() queries
        query.push_str(" LIMIT 1");

        // Print SQL query to logs if debug mode is active
        log::debug!("SQL: {}", query);

        // Execute query and fetch exactly one result
        let row = sqlx::query_with(&query, args).fetch_one(self.tx.executor()).await?;
        R::from_any_row(&row)
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
        // Apply default soft delete filter if not disabled
        if !self.with_deleted {
            if let Some(soft_delete_col) = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name) {
                self = self.is_null(soft_delete_col);
            }
        }

        // Build SELECT clause
        let mut query = String::from("SELECT ");

        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        if self.select_columns.is_empty() {
            return Err(sqlx::Error::ColumnNotFound("is not possible get data without column".to_string()));
        }

        let table_id = self.get_table_identifier();

        let mut select_cols = Vec::with_capacity(self.select_columns.capacity());
        for col in self.select_columns {
            let is_main_col = self.columns.contains(&col.to_snake_case());
            if !self.joins_clauses.is_empty() || self.alias.is_some() {
                if let Some((table, column)) = col.split_once(".") {
                    select_cols.push(format!("\"{}\".\"{}\"", table, column));
                } else if col.contains('(') {
                    select_cols.push(col);
                } else if is_main_col {
                    select_cols.push(format!("\"{}\".\"{}\"", table_id, col));
                } else {
                    select_cols.push(format!("\"{}\"", col));
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
        if let Some(alias) = &self.alias {
            query.push_str(&format!("{} ", alias));
        }

        if !self.joins_clauses.is_empty() {
            query.push_str(&self.joins_clauses.join(" "));
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.driver, &mut arg_counter);
        }

        // Apply GROUP BY
        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        // Apply HAVING
        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }
        }

        // Apply ORDER BY
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", &self.order_clauses.join(", ")));
        }

        // Always add LIMIT 1 for scalar queries
        query.push_str(" LIMIT 1");

        // Print SQL query to logs if debug mode is active
        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

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
    pub fn update<'b, V>(&'b mut self, col: &str, value: V) -> BoxFuture<'b, Result<u64, sqlx::Error>>
    where
        V: ToString + Send + Sync,
    {
        let mut map = std::collections::HashMap::new();
        map.insert(col.to_string(), value.to_string());
        self.execute_update(map)
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
    pub fn updates<'b>(&'b mut self, model: &T) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        self.execute_update(model.to_map())
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
    pub fn update_partial<'b, P: AnyImpl>(&'b mut self, partial: &P) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        self.execute_update(partial.to_map())
    }

    /// Internal helper to execute an UPDATE query from a map of values.
    fn execute_update<'b>(
        &'b mut self,
        data_map: std::collections::HashMap<String, String>,
    ) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        // Apply default soft delete filter if not disabled
        if !self.with_deleted {
            if let Some(soft_delete_col) = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name) {
                let col_owned = soft_delete_col.to_string();
                let clause: FilterFn = Box::new(move |query, _args, _driver, _arg_counter| {
                    query.push_str(" AND ");
                    query.push_str(&format!("\"{}\"", col_owned));
                    query.push_str(" IS NULL");
                });
                self.where_clauses.push(clause);
            }
        }

        Box::pin(async move {
            let table_name = self.table_name.to_snake_case();
            let mut query = format!("UPDATE \"{}\" ", table_name);
            if let Some(alias) = &self.alias {
                query.push_str(&format!("{} ", alias));
            }
            query.push_str("SET ");

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
                let placeholder = match self.driver {
                    Drivers::Postgres => {
                        let idx = arg_counter;
                        arg_counter += 1;

                        if temporal::is_temporal_type(sql_type) {
                            format!("${}{}", idx, temporal::get_postgres_type_cast(sql_type))
                        } else {
                            match sql_type {
                                "UUID" => format!("${}::UUID", idx),
                                "JSONB" | "jsonb" => format!("${}::JSONB", idx),
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
                if args.bind_value(&val_str, sql_type, &self.driver).is_err() {
                    let _ = args.add(val_str);
                }
            }

            // Apply WHERE clauses (appending to args and query)
            for clause in &self.where_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }

            // Print SQL query to logs if debug mode is active
            if self.debug_mode {
                log::debug!("SQL: {}", query);
            }

            // Execute the UPDATE query
            let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;

            Ok(result.rows_affected())
        })
    }

    /// Executes a DELETE query based on the current filters.
    ///
    /// If the model has a `#[orm(soft_delete)]` column, this method performs
    /// an UPDATE setting the soft delete column to the current timestamp instead
    /// of physically deleting the record.
    ///
    /// For permanent deletion, use `hard_delete()`.
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows deleted (or soft-deleted)
    /// * `Err(sqlx::Error)` - Database error
    pub async fn delete(mut self) -> Result<u64, sqlx::Error> {
        // Check for soft delete column
        let soft_delete_col = self.columns_info.iter().find(|c| c.soft_delete).map(|c| c.name);

        if let Some(col) = soft_delete_col {
            // Soft Delete: Update the column to current timestamp
            let table_name = self.table_name.to_snake_case();
            let mut query = format!("UPDATE \"{}\" ", table_name);
            if let Some(alias) = &self.alias {
                query.push_str(&format!("{} ", alias));
            }
            query.push_str(&format!("SET \"{}\" = ", col));

            match self.driver {
                Drivers::Postgres => query.push_str("NOW()"),
                Drivers::SQLite => query.push_str("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"),
                Drivers::MySQL => query.push_str("NOW()"),
            }

            query.push_str(" WHERE 1=1");

            let mut args = AnyArguments::default();
            let mut arg_counter = 1;

            // Apply filters
            for clause in &self.where_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }

            // Print SQL query to logs if debug mode is active
            if self.debug_mode {
                log::debug!("SQL: {}", query);
            }

            let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;
            Ok(result.rows_affected())
        } else {
            // Standard Delete (no soft delete column)
            let mut query = String::from("DELETE FROM \"");
            query.push_str(&self.table_name.to_snake_case());
            query.push_str("\" WHERE 1=1");

            let mut args = AnyArguments::default();
            let mut arg_counter = 1;

            for clause in &self.where_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }

            // Print SQL query to logs if debug mode is active
            if self.debug_mode {
                log::debug!("SQL: {}", query);
            }

            let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;
            Ok(result.rows_affected())
        }
    }

    /// Permanently removes records from the database.
    ///
    /// This method performs a physical DELETE, bypassing any soft delete logic.
    /// Use this when you need to permanently remove records.
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows deleted
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Permanently delete soft-deleted records older than 30 days
    /// db.model::<User>()
    ///     .with_deleted()
    ///     .filter("deleted_at", "<", thirty_days_ago)
    ///     .hard_delete()
    ///     .await?;
    /// ```
    pub async fn hard_delete(mut self) -> Result<u64, sqlx::Error> {
        let mut query = String::from("DELETE FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");

        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.driver, &mut arg_counter);
        }

        // Print SQL query to logs if debug mode is active
        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let result = sqlx::query_with(&query, args).execute(self.tx.executor()).await?;
        Ok(result.rows_affected())
    }
}
