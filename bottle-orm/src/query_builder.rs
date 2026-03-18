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
//! 
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
use sqlx::{Any, Arguments, Decode, Encode, Type, any::AnyArguments};
use std::marker::PhantomData;
use std::collections::{HashMap, HashSet};


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
// Update Value Traits
// ============================================================================

/// Trait for types that can be converted to an optional string value for SQL updates.
///
/// This trait is used by the `update` method to handle both direct values
/// (e.g., `update("role", "admin")`) and NULL values (e.g., `update("role", None::<String>)`).
pub trait ToUpdateValue {
    /// Converts the value to an `Option<String>` for SQL binding.
    ///
    /// # Returns
    ///
    /// * `Some(String)` - String representation for a database value
    /// * `None` - Represents a SQL `NULL`
    fn to_update_value(self) -> Option<String>;
}

macro_rules! impl_update_value {
    ($($t:ty),*) => {
        $(
            impl ToUpdateValue for $t {
                fn to_update_value(self) -> Option<String> {
                    Some(self.to_string())
                }
            }
            impl ToUpdateValue for Option<$t> {
                fn to_update_value(self) -> Option<String> {
                    self.map(|v| v.to_string())
                }
            }
        )*
    }
}

impl_update_value!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, bool, String, &str, uuid::Uuid);
impl_update_value!(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::FixedOffset>, chrono::NaiveDateTime, chrono::NaiveDate, chrono::NaiveTime);

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
    /// SQL BETWEEN
    Between,
    /// SQL NOT BETWEEN
    NotBetween,
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
            Op::Between => "BETWEEN",
            Op::NotBetween => "NOT BETWEEN",
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
pub struct QueryBuilder<T, E> {
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
    pub where_clauses: Vec<FilterFn>,

    /// Collection of ORDER BY clauses
    pub order_clauses: Vec<String>,

    /// Collection of JOIN clause to filter entry tables
    pub joins_clauses: Vec<FilterFn>,

    /// Collection of relations to eager load
    pub with_relations: Vec<String>,

    /// Modifiers for eager loading relations
    pub with_modifiers: std::collections::HashMap<String, std::sync::Arc<dyn std::any::Any + Send + Sync>>,

    /// Map of table names to their aliases used in JOINS
    pub join_aliases: std::collections::HashMap<String, String>,

    /// Maximum number of rows to return (LIMIT)
    pub limit: Option<usize>,

    /// Number of rows to skip (OFFSET)
    pub offset: Option<usize>,

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

    /// UNION and UNION ALL clauses
    pub(crate) union_clauses: Vec<(String, FilterFn)>,

    /// PhantomData to bind the generic type T
    pub(crate) _marker: PhantomData<T>,
}

// ============================================================================
// QueryBuilder Implementation
// ============================================================================

/// A wrapper for relation query modifiers to allow storage in Any-based collections.
pub struct QueryModifier {
    pub modifier: std::sync::Arc<dyn Fn(QueryBuilder<crate::any_struct::AnyImplStruct, crate::Database>) -> QueryBuilder<crate::any_struct::AnyImplStruct, crate::Database> + Send + Sync + 'static>,
}

impl<T, E> QueryBuilder<T, E>
where
    T: Model + Send + Sync + Unpin + AnyImpl,
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
    /// ```rust
    /// # use bottle_orm::{Database, Model};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     username: String,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// // Usually called via db.model::<User>()
    /// let query = db.model::<User>();
    /// #     Ok(())
    /// # }
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
            union_clauses: Vec::new(),
            with_relations: Vec::new(),
            with_modifiers: std::collections::HashMap::new(),
            _marker: PhantomData,
        }
    }

    /// Returns the table name or alias if set.
    pub(crate) fn get_table_identifier(&self) -> String {
        self.alias.clone().unwrap_or_else(|| self.table_name.to_snake_case())
    }

    /// Adds a relation to be eager loaded with the query results.
    ///
    /// Eager loading allows you to fetch related models in a single operation
    /// (typically using a second optimized query) to avoid the N+1 query problem.
    ///
    /// # Arguments
    ///
    /// * `relation` - The name of the relation to load (must match the field name in the model)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users = db.model::<User>()
    ///     .with("posts")
    ///     .scan()
    ///     .await?;
    ///
    /// for user in users {
    ///     println!("User {} has {} posts", user.username, user.posts.len());
    /// }
    /// ```
    pub fn with(mut self, relation: &str) -> Self {
        self.with_relations.push(relation.to_string());
        self
    }

    /// Adds a relation to be eager loaded with a custom query modifier.
    ///
    /// This allows you to apply filters, ordering, and pagination to the
    /// related models.
    ///
    /// # Arguments
    ///
    /// * `relation` - The name of the relation to load
    /// * `modifier` - A closure that receives a QueryBuilder and returns it modified
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users = db.model::<User>()
    ///     .with_query("posts", |query| {
    ///         query.filter("status", Op::Eq, "published").limit(5)
    ///     })
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn with_query<F>(mut self, relation: &str, modifier: F) -> Self
    where
        F: Fn(QueryBuilder<crate::any_struct::AnyImplStruct, crate::Database>) -> QueryBuilder<crate::any_struct::AnyImplStruct, crate::Database> + Send + Sync + 'static,
    {
        self.with_relations.push(relation.to_string());
        let arc_mod = std::sync::Arc::new(modifier);
        let wrapper = QueryModifier { modifier: arc_mod };
        self.with_modifiers.insert(relation.to_string(), std::sync::Arc::new(wrapper));
        self
    }

    // ========================================================================
    // Query Building Methods
    // ========================================================================

    /// Internal helper to add a WHERE clause with a specific join operator.
    fn filter_internal<V>(mut self, joiner: &str, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let op_str = op.as_sql();
        let table_id = self.get_table_identifier();
        // Check if the column exists in the main table to avoid ambiguous references in JOINS
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let joiner_owned = joiner.to_string();
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(&joiner_owned);
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

    /// Adds a WHERE IN (SUBQUERY) clause to the query.
    ///
    /// This allows for filtering a column based on the results of another query.
    ///
    /// # Example
    /// ```rust,ignore
    /// let subquery = db.model::<Post>().select("user_id").filter("views", ">", 1000);
    /// db.model::<User>().filter_subquery("id", Op::In, subquery).scan().await?;
    /// ```
    pub fn filter_subquery<S, SE>(mut self, col: &'static str, op: Op, mut subquery: QueryBuilder<S, SE>) -> Self
    where
        S: Model + Send + Sync + Unpin + AnyImpl + 'static,
        SE: Connection + 'static,
    {
        subquery.apply_soft_delete_filter();
        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let op_str = op.as_sql();

        let clause: FilterFn = Box::new(move |query, args, _driver, arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(&format!(" {} (", op_str));

            subquery.write_select_sql::<S>(query, args, arg_counter);
            query.push_str(")");
        });

        self.where_clauses.push(clause);
        self
    }

    /// Truncates the table associated with this Model.
    ///
    /// This method removes all records from the table. It uses `TRUNCATE TABLE`
    /// for Postgres and MySQL, and `DELETE FROM` with sequence reset for SQLite.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Table truncated successfully
    /// * `Err(sqlx::Error)` - Database error occurred
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<Log>().truncate().await?;
    /// ```
    pub async fn truncate(self) -> Result<(), sqlx::Error> {
        let table_name = self.table_name.to_snake_case();
        let query = match self.driver {
            Drivers::Postgres | Drivers::MySQL => format!("TRUNCATE TABLE \"{}\"", table_name),
            Drivers::SQLite => format!("DELETE FROM \"{}\"", table_name),
        };

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        self.tx.execute(&query, AnyArguments::default()).await?;
        
        // For SQLite, reset auto-increment if exists
        if matches!(self.driver, Drivers::SQLite) {
            let _ = self.tx.execute(&format!("DELETE FROM sqlite_sequence WHERE name=\"{}\"", table_name), AnyArguments::default()).await;
        }

        Ok(())
    }

    /// Combines the results of this query with another query using UNION.
    ///
    /// This method allows you to combine the result sets of two queries into a single
    /// result set. Duplicate rows are removed by default.
    ///
    /// # Arguments
    ///
    /// * `other` - Another QueryBuilder instance to combine with.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let q1 = db.model::<User>().filter("age", ">", 18);
    /// let q2 = db.model::<User>().filter("status", "=", "premium");
    /// let results = q1.union(q2).scan().await?;
    /// ```
    pub fn union(self, other: QueryBuilder<T, E>) -> Self where T: AnyImpl + 'static, E: 'static {
        self.union_internal("UNION", other)
    }

    /// Combines the results of this query with another query using UNION ALL.
    ///
    /// This method allows you to combine the result sets of two queries into a single
    /// result set, including all duplicates.
    ///
    /// # Arguments
    ///
    /// * `other` - Another QueryBuilder instance to combine with.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let q1 = db.model::<User>().filter("age", ">", 18);
    /// let q2 = db.model::<User>().filter("status", "=", "premium");
    /// let results = q1.union_all(q2).scan().await?;
    /// ```
    pub fn union_all(self, other: QueryBuilder<T, E>) -> Self where T: AnyImpl + 'static, E: 'static {
        self.union_internal("UNION ALL", other)
    }

    fn union_internal(mut self, op: &str, mut other: QueryBuilder<T, E>) -> Self where T: AnyImpl + 'static, E: 'static {
        other.apply_soft_delete_filter();
        let op_owned = op.to_string();
        
        self.union_clauses.push((op_owned.clone(), Box::new(move |query: &mut String, args: &mut AnyArguments<'_>, _driver: &Drivers, arg_counter: &mut usize| {
            query.push_str(" ");
            query.push_str(&op_owned);
            query.push_str(" ");
            other.write_select_sql::<T>(query, args, arg_counter);
        })));
        self
    }

    /// Internal helper to write the SELECT SQL to a string buffer.
    pub(crate) fn write_select_sql<R: AnyImpl>(
        &self,
        query: &mut String,
        args: &mut AnyArguments,
        arg_counter: &mut usize,
    ) {
        query.push_str("SELECT ");

        if self.is_distinct {
            query.push_str("DISTINCT ");
        }

        query.push_str(&self.select_args_sql::<R>().join(", "));

        // Build FROM clause
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" ");
        if let Some(alias) = &self.alias {
            query.push_str(&format!("\"{}\" ", alias));
        }

        if !self.joins_clauses.is_empty() {
            for join_clause in &self.joins_clauses {
                query.push(' ');
                join_clause(query, args, &self.driver, arg_counter);
            }
        }

        query.push_str(" WHERE 1=1");

        // Apply WHERE clauses
        for clause in &self.where_clauses {
            clause(query, args, &self.driver, arg_counter);
        }

        // Apply GROUP BY
        if !self.group_by_clauses.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by_clauses.join(", ")));
        }

        // Apply HAVING
        if !self.having_clauses.is_empty() {
            query.push_str(" HAVING 1=1");
            for clause in &self.having_clauses {
                clause(query, args, &self.driver, arg_counter);
            }
        }

        // Apply ORDER BY clauses
        if !self.order_clauses.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_clauses.join(", ")));
        }

        // Apply LIMIT clause
        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ");
            match self.driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    *arg_counter += 1;
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
                    *arg_counter += 1;
                }
                _ => query.push('?'),
            }
            let _ = args.add(offset as i64);
        }

        // Apply UNION clauses
        for (_op, clause) in &self.union_clauses {
            clause(query, args, &self.driver, arg_counter);
        }
    }

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
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     age: i32,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>().filter("age", Op::Gte, 18);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn filter<V>(self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter_internal(" AND ", col, op, value)
    }

    /// Adds an OR WHERE clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to filter on
    /// * `op` - The comparison operator
    /// * `value` - The value to compare against
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     age: i32,
    /// #     active: bool,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>()
    ///     .filter("age", Op::Lt, 18)
    ///     .or_filter("active", Op::Eq, false);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn or_filter<V>(self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter_internal(" OR ", col, op, value)
    }

    /// Adds an AND NOT WHERE clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to filter on
    /// * `op` - The comparison operator
    /// * `value` - The value to compare against
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     status: String,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>().not_filter("status", Op::Eq, "banned".to_string());
    /// #     Ok(())
    /// # }
    /// ```
    pub fn not_filter<V>(self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter_internal(" AND NOT ", col, op, value)
    }

    /// Adds an OR NOT WHERE clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to filter on
    /// * `op` - The comparison operator
    /// * `value` - The value to compare against
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     age: i32,
    /// #     status: String,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>()
    ///     .filter("age", Op::Gt, 18)
    ///     .or_not_filter("status", Op::Eq, "inactive".to_string());
    /// #     Ok(())
    /// # }
    /// ```
    pub fn or_not_filter<V>(self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.filter_internal(" OR NOT ", col, op, value)
    }

    /// Adds a BETWEEN clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name
    /// * `start` - The start value of the range
    /// * `end` - The end value of the range
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     age: i32,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>().between("age", 18, 30);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn between<V>(mut self, col: &'static str, start: V, end: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(" BETWEEN ");

            match driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${} AND ${}", arg_counter, *arg_counter + 1));
                    *arg_counter += 2;
                }
                _ => query.push_str("? AND ?"),
            }

            let _ = args.add(start.clone());
            let _ = args.add(end.clone());
        });
        self.where_clauses.push(clause);
        self
    }

    /// Adds an OR BETWEEN clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name
    /// * `start` - The start value of the range
    /// * `end` - The end value of the range
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     age: i32,
    /// #     salary: i32,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>()
    ///     .between("age", 18, 30)
    ///     .or_between("salary", 5000, 10000);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn or_between<V>(mut self, col: &'static str, start: V, end: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" OR ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(" BETWEEN ");

            match driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${} AND ${}", arg_counter, *arg_counter + 1));
                    *arg_counter += 2;
                }
                _ => query.push_str("? AND ?"),
            }

            let _ = args.add(start.clone());
            let _ = args.add(end.clone());
        });
        self.where_clauses.push(clause);
        self
    }

    /// Adds an IN list clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name
    /// * `values` - A vector of values
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     status: String,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>().in_list("status", vec!["active".to_string(), "pending".to_string()]);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn in_list<V>(mut self, col: &'static str, values: Vec<V>) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        if values.is_empty() {
            // WHERE 1=0 to ensure empty result
            let clause: FilterFn = Box::new(|query, _, _, _| {
                query.push_str(" AND 1=0");
            });
            self.where_clauses.push(clause);
            return self;
        }

        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(" IN (");

            let mut placeholders = Vec::new();
            for _ in &values {
                match driver {
                    Drivers::Postgres => {
                        placeholders.push(format!("${}", arg_counter));
                        *arg_counter += 1;
                    }
                    _ => placeholders.push("?".to_string()),
                }
            }
            query.push_str(&placeholders.join(", "));
            query.push(')');

            for val in &values {
                let _ = args.add(val.clone());
            }
        });
        self.where_clauses.push(clause);
        self
    }

    /// Adds an OR IN list clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name
    /// * `values` - A vector of values
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bottle_orm::{Database, Model, Op};
    /// # #[derive(Model, Debug, Clone)]
    /// # struct User {
    /// #     #[orm(primary_key)]
    /// #     id: i32,
    /// #     status: String,
    /// #     role: String,
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// #     let db = Database::connect("sqlite::memory:").await?;
    /// let query = db.model::<User>()
    ///     .filter("status", Op::Eq, "active".to_string())
    ///     .or_in_list("role", vec!["admin".to_string(), "editor".to_string()]);
    /// #     Ok(())
    /// # }
    /// ```
    pub fn or_in_list<V>(mut self, col: &'static str, values: Vec<V>) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        if values.is_empty() {
            return self;
        }

        let table_id = self.get_table_identifier();
        let is_main_col = self.columns.contains(&col.to_snake_case());
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" OR ");
            if let Some((table, column)) = col.split_once(".") {
                query.push_str(&format!("\"{}\".\"{}\"", table, column));
            } else if is_main_col {
                query.push_str(&format!("\"{}\".\"{}\"", table_id, col));
            } else {
                query.push_str(&format!("\"{}\"", col));
            }
            query.push_str(" IN (");

            let mut placeholders = Vec::new();
            for _ in &values {
                match driver {
                    Drivers::Postgres => {
                        placeholders.push(format!("${}", arg_counter));
                        *arg_counter += 1;
                    }
                    _ => placeholders.push("?".to_string()),
                }
            }
            query.push_str(&placeholders.join(", "));
            query.push(')');

            for val in &values {
                let _ = args.add(val.clone());
            }
        });
        self.where_clauses.push(clause);
        self
    }

    /// Groups filters inside parentheses with an AND operator.
    ///
    /// This allows for constructing complex WHERE clauses with nested logic.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that receives a `QueryBuilder` and returns it with more filters
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("active", Op::Eq, true)
    ///     .group(|q| q.filter("age", Op::Gt, 18).or_filter("role", Op::Eq, "admin"))
    ///     .scan()
    ///     .await?;
    /// // SQL: AND "active" = true AND (1=1 AND ("age" > 18 OR "role" = 'admin'))
    /// ```
    pub fn group<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        let old_clauses = std::mem::take(&mut self.where_clauses);
        self = f(self);
        let group_clauses = std::mem::take(&mut self.where_clauses);
        self.where_clauses = old_clauses;

        if !group_clauses.is_empty() {
            let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
                query.push_str(" AND (1=1");
                for c in &group_clauses {
                    c(query, args, driver, arg_counter);
                }
                query.push_str(")");
            });
            self.where_clauses.push(clause);
        }
        self
    }

    /// Groups filters inside parentheses with an OR operator.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that receives a `QueryBuilder` and returns it with more filters
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("active", Op::Eq, true)
    ///     .or_group(|q| q.filter("role", Op::Eq, "admin").filter("age", Op::Gt, 18))
    ///     .scan()
    ///     .await?;
    /// // SQL: AND "active" = true OR (1=1 AND ("role" = 'admin' AND "age" > 18))
    /// ```
    pub fn or_group<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        let old_clauses = std::mem::take(&mut self.where_clauses);
        self = f(self);
        let group_clauses = std::mem::take(&mut self.where_clauses);
        self.where_clauses = old_clauses;

        if !group_clauses.is_empty() {
            let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
                query.push_str(" OR (1=1");
                for c in &group_clauses {
                    c(query, args, driver, arg_counter);
                }
                query.push_str(")");
            });
            self.where_clauses.push(clause);
        }
        self
    }

    /// Adds a raw WHERE clause with a placeholder and a single value.
    ///
    /// This allows writing raw SQL conditions with a `?` placeholder.
    /// To use multiple placeholders with different types, chain multiple `where_raw` calls.
    ///
    /// # Arguments
    ///
    /// * `sql` - Raw SQL string with one `?` placeholder (e.g., "age > ?")
    /// * `value` - Value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .where_raw("name = ?", "Alice".to_string())
    ///     .where_raw("age >= ?", 18)
    ///     .scan()
    ///     .await?;
    /// // SQL: AND name = 'Alice' AND age >= 18
    /// ```
    pub fn where_raw<V>(mut self, sql: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.where_clauses.push(self.create_raw_clause(" AND ", sql, value));
        self
    }

    /// Adds a raw OR WHERE clause with a placeholder.
    ///
    /// # Arguments
    ///
    /// * `sql` - Raw SQL string with one `?` placeholder
    /// * `value` - Value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("active", Op::Eq, true)
    ///     .or_where_raw("age > ?", 18)
    ///     .scan()
    ///     .await?;
    /// // SQL: AND "active" = true OR age > 18
    /// ```
    pub fn or_where_raw<V>(mut self, sql: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.where_clauses.push(self.create_raw_clause(" OR ", sql, value));
        self
    }

    /// Internal helper to create a raw SQL clause with a single value.
    fn create_raw_clause<V>(&self, joiner: &'static str, sql: &str, value: V) -> FilterFn
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let sql_owned = sql.to_string();
        Box::new(move |query, args, driver, arg_counter| {
            query.push_str(joiner);
            
            let mut processed_sql = sql_owned.clone();
            
            // If no placeholder is found, try to be helpful
            if !processed_sql.contains('?') {
                let trimmed = processed_sql.trim();
                if trimmed.ends_with('=') || trimmed.ends_with('>') || trimmed.ends_with('<') || trimmed.to_uppercase().ends_with(" LIKE") {
                    processed_sql.push_str(" ?");
                } else if !trimmed.contains(' ') && !trimmed.contains('(') {
                    // It looks like just a column name
                    processed_sql.push_str(" = ?");
                }
            }

            // Replace '?' with driver-specific placeholders only if needed
            if matches!(driver, Drivers::Postgres) {
                while let Some(pos) = processed_sql.find('?') {
                    let placeholder = format!("${}", arg_counter);
                    *arg_counter += 1;
                    processed_sql.replace_range(pos..pos + 1, &placeholder);
                }
            }
            
            query.push_str(&processed_sql);
            let _ = args.add(value.clone());
        })
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

    /// Adds an INNER JOIN clause to the query.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join (with optional alias)
    /// * `on` - The join condition (e.g., "users.id = posts.user_id")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .join("posts p", "u.id = p.user_id")
    ///     .scan()
    ///     .await?;
    /// // SQL: INNER JOIN "posts" p ON u.id = p.user_id
    /// ```
    pub fn join(self, table: &str, s_query: &str) -> Self {
        self.join_generic("", table, s_query)
    }

    /// Internal helper for specific join types
    fn join_generic(mut self, join_type: &str, table: &str, s_query: &str) -> Self {
        let table_owned = table.to_string();
        let join_type_owned = join_type.to_string();
        
        let trimmed_value = s_query.replace(" ", "");
        let values = trimmed_value.split_once("=");
        let mut parsed_query = s_query.to_string();
        
        if let Some((first, second)) = values {
            // Try to parse table.column = table.column
            if let Some((t1, c1)) = first.split_once('.') {
                if let Some((t2, c2)) = second.split_once('.') {
                    parsed_query = format!("\"{}\".\"{}\" = \"{}\".\"{}\"", t1, c1, t2, c2);
                }
            }
        }

        if let Some((table_name, alias)) = table.split_once(" ") {
            self.join_aliases.insert(table_name.to_snake_case(), alias.to_string());
        } else {
            self.join_aliases.insert(table.to_snake_case(), table.to_string());
        }

        self.joins_clauses.push(Box::new(move |query, _args, _driver, _arg_counter| {
            if let Some((table_name, alias)) = table_owned.split_once(" ") {
                query.push_str(&format!("{} JOIN \"{}\" \"{}\" ON {}", join_type_owned, table_name, alias, parsed_query));
            } else {
                query.push_str(&format!("{} JOIN \"{}\" ON {}", join_type_owned, table_owned, parsed_query));
            }
        }));
        self
    }

    /// Adds a JOIN clause with a placeholder and a bound value.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join
    /// * `on` - The join condition with a `?` placeholder
    /// * `value` - The value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .join_raw("posts p", "p.user_id = u.id AND p.status = ?", "published")
    ///     .scan()
    ///     .await?;
    /// // SQL: JOIN "posts" p ON p.user_id = u.id AND p.status = 'published'
    /// ```
    pub fn join_raw<V>(self, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.join_generic_raw("", table, on, value)
    }

    /// Adds a raw LEFT JOIN clause with a placeholder and a bound value.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join (with optional alias)
    /// * `on` - The join condition with a `?` placeholder
    /// * `value` - The value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.left_join_raw("posts p", "p.user_id = u.id AND p.status = ?", "published")
    /// ```
    pub fn left_join_raw<V>(self, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.join_generic_raw("LEFT", table, on, value)
    }

    /// Adds a raw RIGHT JOIN clause with a placeholder and a bound value.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join (with optional alias)
    /// * `on` - The join condition with a `?` placeholder
    /// * `value` - The value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.right_join_raw("users u", "u.id = p.user_id AND u.active = ?", true)
    /// ```
    pub fn right_join_raw<V>(self, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.join_generic_raw("RIGHT", table, on, value)
    }

    /// Adds a raw INNER JOIN clause with a placeholder and a bound value.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join (with optional alias)
    /// * `on` - The join condition with a `?` placeholder
    /// * `value` - The value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.inner_join_raw("accounts a", "a.user_id = u.id AND a.type = ?", "checking")
    /// ```
    pub fn inner_join_raw<V>(self, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.join_generic_raw("INNER", table, on, value)
    }

    /// Adds a raw FULL JOIN clause with a placeholder and a bound value.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join (with optional alias)
    /// * `on` - The join condition with a `?` placeholder
    /// * `value` - The value to bind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.full_join_raw("profiles pr", "pr.user_id = u.id AND pr.verified = ?", true)
    /// ```
    pub fn full_join_raw<V>(self, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.join_generic_raw("FULL", table, on, value)
    }

    /// Internal helper for raw join types
    fn join_generic_raw<V>(mut self, join_type: &str, table: &str, on: &str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let table_owned = table.to_string();
        let on_owned = on.to_string();
        let join_type_owned = join_type.to_string();
        
        if let Some((table_name, alias)) = table.split_once(" ") {
            self.join_aliases.insert(table_name.to_snake_case(), alias.to_string());
        } else {
            self.join_aliases.insert(table.to_snake_case(), table.to_string());
        }

        self.joins_clauses.push(Box::new(move |query, args, driver, arg_counter| {
            if let Some((table_name, alias)) = table_owned.split_once(" ") {
                query.push_str(&format!("{} JOIN \"{}\" {} ON ", join_type_owned, table_name, alias));
            } else {
                query.push_str(&format!("{} JOIN \"{}\" ON ", join_type_owned, table_owned));
            }

            let mut processed_on = on_owned.clone();
            if let Some(pos) = processed_on.find('?') {
                let placeholder = match driver {
                    Drivers::Postgres => {
                        let p = format!("${}", arg_counter);
                        *arg_counter += 1;
                        p
                    }
                    _ => "?".to_string(),
                };
                processed_on.replace_range(pos..pos + 1, &placeholder);
            }
            
            query.push_str(&processed_on);
            let _ = args.add(value.clone());
        }));
        self
    }

    /// Adds a LEFT JOIN clause.
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
    ///     .left_join("posts p", "u.id = p.user_id")
    ///     .scan()
    ///     .await?;
    /// // SQL: LEFT JOIN "posts" p ON u.id = p.user_id
    /// ```
    pub fn left_join(self, table: &str, on: &str) -> Self {
        self.join_generic("LEFT", table, on)
    }

    /// Adds a RIGHT JOIN clause.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<Post>()
    ///     .right_join("users u", "p.user_id = u.id")
    ///     .scan()
    ///     .await?;
    /// // SQL: RIGHT JOIN "users" u ON p.user_id = u.id
    /// ```
    pub fn right_join(self, table: &str, on: &str) -> Self {
        self.join_generic("RIGHT", table, on)
    }

    /// Adds an INNER JOIN clause.
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
    ///     .inner_join("posts p", "u.id = p.user_id")
    ///     .scan()
    ///     .await?;
    /// // SQL: INNER JOIN "posts" p ON u.id = p.user_id
    /// ```
    pub fn inner_join(self, table: &str, on: &str) -> Self {
        self.join_generic("INNER", table, on)
    }

    /// Adds a FULL JOIN clause.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table to join with
    /// * `on` - The join condition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// query.full_join("profiles pr", "u.id = pr.user_id")
    /// // SQL: FULL JOIN "profiles" pr ON u.id = pr.user_id
    /// ```
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
    ///     .having("COUNT(*)", Op::Gt, 5)
    ///     .scan()
    ///     .await?;
    /// ```
    pub fn having<V>(mut self, col: &'static str, op: Op, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        let op_str = op.as_sql();
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND ");
            query.push_str(col);
            query.push(' ');
            query.push_str(op_str);
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
        N: FromAnyRow + AnyImpl + for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        let quoted_col = if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            format!("\"{}\".\"{}\"", parts[0].trim_matches('"'), parts[1].trim_matches('"'))
        } else {
            format!("\"{}\"", column.trim_matches('"'))
        };
        self.select_columns = vec![format!("SUM({})", quoted_col)];
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
        N: FromAnyRow + AnyImpl + for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        let quoted_col = if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            format!("\"{}\".\"{}\"", parts[0].trim_matches('"'), parts[1].trim_matches('"'))
        } else {
            format!("\"{}\"", column.trim_matches('"'))
        };
        self.select_columns = vec![format!("AVG({})", quoted_col)];
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
        N: FromAnyRow + AnyImpl + for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        let quoted_col = if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            format!("\"{}\".\"{}\"", parts[0].trim_matches('"'), parts[1].trim_matches('"'))
        } else {
            format!("\"{}\"", column.trim_matches('"'))
        };
        self.select_columns = vec![format!("MIN({})", quoted_col)];
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
        N: FromAnyRow + AnyImpl + for<'r> Decode<'r, Any> + Type<Any> + Send + Unpin,
    {
        let quoted_col = if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            format!("\"{}\".\"{}\"", parts[0].trim_matches('"'), parts[1].trim_matches('"'))
        } else {
            format!("\"{}\"", column.trim_matches('"'))
        };
        self.select_columns = vec![format!("MAX({})", quoted_col)];
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
    /// 
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
            let data_map = Model::to_map(model);

            // Early return if no data to insert
            if data_map.is_empty() {
                return Ok(());
            }

            let table_name = self.table_name.to_snake_case();
            let columns_info = <T as Model>::columns();

            let mut target_columns = Vec::new();
            let mut bindings: Vec<(Option<String>, &str)> = Vec::new();

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
                                s if s.ends_with("[]") => format!("${}::{}", idx, s),
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

            if self.debug_mode {
                log::debug!("SQL: {}", query_str);
            }

            let mut args = AnyArguments::default();

            // Bind values using the optimized value_binding module
            for (val_opt, sql_type) in bindings {
                if let Some(val_str) = val_opt {
                    if args.bind_value(&val_str, sql_type, &self.driver).is_err() {
                        let _ = args.add(val_str);
                    }
                } else {
                    let _ = args.add(None::<String>);
                }
            }

            // Execute the INSERT query
            self.tx.execute(&query_str, args).await?;
            Ok(())
        })
    }

    /// Inserts multiple records into the database in a single batch operation.
    ///
    /// This is significantly faster than performing individual inserts in a loop
    /// as it generates a single SQL statement with multiple VALUES groups.
    ///
    /// # Type Binding Strategy
    ///
    /// Similar to the single record `insert`, this method uses string parsing for
    /// type binding. It ensures that all columns defined in the model are included
    /// in the insert statement, providing NULL for any missing optional values.
    ///
    /// # Arguments
    ///
    /// * `models` - A slice of model instances to insert
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully inserted all records
    /// * `Err(sqlx::Error)` - Database error during insertion
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users = vec![
    ///     User { username: "alice".to_string(), ... },
    ///     User { username: "bob".to_string(), ... },
    /// ];
    ///
    /// db.model::<User>().batch_insert(&users).await?;
    /// ```
    pub fn batch_insert<'b>(&'b mut self, models: &'b [T]) -> BoxFuture<'b, Result<(), sqlx::Error>> {
        Box::pin(async move {
            if models.is_empty() {
                return Ok(());
            }

            let table_name = self.table_name.to_snake_case();
            let columns_info = <T as Model>::columns();

            // Collect all column names for the INSERT statement
            // We use all columns defined in the model to ensure consistency across the batch
            let target_columns: Vec<String> = columns_info
                .iter()
                .map(|c| {
                    let col_name_clean = c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case();
                    format!("\"{}\"", col_name_clean)
                })
                .collect();

            let mut value_groups = Vec::new();
            let mut bind_index = 1;

            // Generate placeholders for all models
            for _ in models {
                let mut placeholders = Vec::new();
                for col in &columns_info {
                    match self.driver {
                        Drivers::Postgres => {
                            let p = if temporal::is_temporal_type(col.sql_type) {
                                format!("${}{}", bind_index, temporal::get_postgres_type_cast(col.sql_type))
                            } else {
                                match col.sql_type {
                                    "UUID" => format!("${}::UUID", bind_index),
                                    "JSONB" | "jsonb" => format!("${}::JSONB", bind_index),
                                    _ => format!("${}", bind_index),
                                }
                            };
                            placeholders.push(p);
                            bind_index += 1;
                        }
                        _ => {
                            placeholders.push("?".to_string());
                        }
                    }
                }
                value_groups.push(format!("({})", placeholders.join(", ")));
            }

            let query_str = format!(
                "INSERT INTO \"{}\" ({}) VALUES {}",
                table_name,
                target_columns.join(", "),
                value_groups.join(", ")
            );

            if self.debug_mode {
                log::debug!("SQL Batch: {}", query_str);
            }

            let mut args = AnyArguments::default();

            for model in models {
                let data_map = Model::to_map(model);
                for col in &columns_info {
                    let val_opt = data_map.get(col.name);
                    let sql_type = col.sql_type;

                    if let Some(Some(val_str)) = val_opt {
                        if args.bind_value(val_str, sql_type, &self.driver).is_err() {
                            let _ = args.add(val_str.clone());
                        }
                    } else {
                        // Bind NULL for missing or None values
                        let _ = args.add(None::<String>);
                    }
                }
            }

            // Execute the batch INSERT query
            self.tx.execute(&query_str, args).await?;
            Ok(())
        })
    }

    /// Inserts a record or updates it if a conflict occurs (UPSERT).
    ///
    /// This method provides a cross-database way to perform "Insert or Update" operations.
    /// It uses `ON CONFLICT` for PostgreSQL and SQLite, and `ON DUPLICATE KEY UPDATE` for MySQL.
    ///
    /// # Arguments
    ///
    /// * `model` - The model instance to insert or update
    /// * `conflict_columns` - Columns that trigger the conflict (e.g., primary key or unique columns)
    /// * `update_columns` - Columns to update when a conflict occurs
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user = User { id: 1, username: "alice".to_string(), age: 25 };
    ///
    /// // If id 1 exists, update username and age
    /// db.model::<User>().upsert(&user, &["id"], &["username", "age"]).await?;
    /// ```
    pub fn upsert<'b>(
        &'b mut self,
        model: &'b T,
        conflict_columns: &'b [&'b str],
        update_columns: &'b [&'b str],
    ) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        Box::pin(async move {
            let data_map = Model::to_map(model);
            if data_map.is_empty() {
                return Ok(0);
            }

            let table_name = self.table_name.to_snake_case();
            let columns_info = <T as Model>::columns();

            let mut target_columns = Vec::new();
            let mut bindings: Vec<(Option<String>, &str)> = Vec::new();

            // Build INSERT part
            for (col_name, value) in &data_map {
                let col_name_clean = col_name.strip_prefix("r#").unwrap_or(col_name).to_snake_case();
                target_columns.push(format!("\"{}\"", col_name_clean));

                let sql_type = columns_info.iter().find(|c| {
                    let c_clean = c.name.strip_prefix("r#").unwrap_or(c.name);
                    c_clean == *col_name || c_clean.to_snake_case() == col_name_clean
                }).map(|c| c.sql_type).unwrap_or("TEXT");
                bindings.push((value.clone(), sql_type));
            }

            let mut arg_counter = 1;
            let mut placeholders = Vec::new();
            for (_, sql_type) in &bindings {
                match self.driver {
                    Drivers::Postgres => {
                        let p = if temporal::is_temporal_type(sql_type) {
                            format!("${}{}", arg_counter, temporal::get_postgres_type_cast(sql_type))
                        } else {
                            match *sql_type {
                                "UUID" => format!("${}::UUID", arg_counter),
                                "JSONB" | "jsonb" => format!("${}::JSONB", arg_counter),
                                _ => format!("${}", arg_counter),
                            }
                        };
                        placeholders.push(p);
                        arg_counter += 1;
                    }
                    _ => {
                        placeholders.push("?".to_string());
                    }
                }
            }

            let mut query_str = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({})",
                table_name,
                target_columns.join(", "),
                placeholders.join(", ")
            );

            // Build Conflict/Update part
            match self.driver {
                Drivers::Postgres | Drivers::SQLite => {
                    let conflict_cols_str = conflict_columns
                        .iter()
                        .map(|c| format!("\"{}\"", c.to_snake_case()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    
                    query_str.push_str(&format!(" ON CONFLICT ({}) DO UPDATE SET ", conflict_cols_str));
                    
                    let mut update_clauses = Vec::new();
                    let mut update_bindings = Vec::new();

                    for col in update_columns {
                        let col_snake = col.to_snake_case();
                        if let Some((_key, val_opt)) = data_map.iter().find(|(k, _)| {
                            let k_clean = k.strip_prefix("r#").unwrap_or(*k);
                            k_clean == *col || k_clean.to_snake_case() == col_snake
                        }) {
                            let sql_type_opt = columns_info.iter().find(|c| {
                                let c_clean = c.name.strip_prefix("r#").unwrap_or(c.name);
                                c_clean == *col || c_clean.to_snake_case() == col_snake
                            }).map(|c| c.sql_type);
                            
                            let sql_type = match sql_type_opt {
                                Some(t) => t,
                                None => continue,
                            };
                            
                            let placeholder = match self.driver {
                                Drivers::Postgres => {
                                    let p = if temporal::is_temporal_type(sql_type) {
                                        format!("${}{}", arg_counter, temporal::get_postgres_type_cast(sql_type))
                                    } else {
                                        match sql_type {
                                            "UUID" => format!("${}::UUID", arg_counter),
                                            "JSONB" | "jsonb" => format!("${}::JSONB", arg_counter),
                                            _ => format!("${}", arg_counter),
                                        }
                                    };
                                    arg_counter += 1;
                                    p
                                }
                                _ => "?".to_string(),
                            };
                            update_clauses.push(format!("\"{}\" = {}", col_snake, placeholder));
                            update_bindings.push((val_opt.clone(), sql_type));
                        }
                    }
                    if update_clauses.is_empty() {
                        query_str.push_str(" NOTHING");
                    } else {
                        query_str.push_str(&update_clauses.join(", "));
                    }
                    bindings.extend(update_bindings);
                }
                Drivers::MySQL => {
                    query_str.push_str(" ON DUPLICATE KEY UPDATE ");
                    let mut update_clauses = Vec::new();
                    for col in update_columns {
                        let col_snake = col.to_snake_case();
                        update_clauses.push(format!("\"{}\" = VALUES(\"{}\")", col_snake, col_snake));
                    }
                    query_str.push_str(&update_clauses.join(", "));
                }
            }

            if self.debug_mode {
                log::debug!("SQL Upsert: {}", query_str);
            }

            let mut args = AnyArguments::default();
            for (val_opt, sql_type) in bindings {
                if let Some(val_str) = val_opt {
                    if args.bind_value(&val_str, sql_type, &self.driver).is_err() {
                        let _ = args.add(val_str);
                    }
                } else {
                    let _ = args.add(None::<String>);
                }
            }

            let result = self.tx.execute(&query_str, args).await?;
            Ok(result.rows_affected())
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
        let mut query = String::new();
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        self.write_select_sql::<T>(&mut query, &mut args, &mut arg_counter);
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
        let main_table_snake = self.table_name.to_snake_case();
        
        if struct_cols.is_empty() {
            if self.select_columns.is_empty() { return vec!["*".to_string()]; }
            
            // If result type is a tuple or primitive (struct_cols is empty),
            // we still need to handle temporal types for Postgres.
            if matches!(self.driver, Drivers::Postgres) {
                let mut args = Vec::new();
                for s in &self.select_columns {
                    for sub in s.split(',') {
                        let s_trim = sub.trim();
                        if s_trim.contains(' ') || s_trim.contains('(') {
                            args.push(s_trim.to_string());
                            continue;
                        }

                        let (t, c) = if let Some((t, c)) = s_trim.split_once('.') {
                            (t.trim().trim_matches('"'), c.trim().trim_matches('"'))
                        } else {
                            (table_id.as_str(), s_trim.trim_matches('"'))
                        };

                        let c_snake = c.to_snake_case();
                        let mut is_temporal = false;
                        
                        // Check if this column is known to be temporal
                        if let Some(info) = self.columns_info.iter().find(|info| {
                            info.name.to_snake_case() == c_snake
                        }) {
                            if is_temporal_type(info.sql_type) {
                                is_temporal = true;
                            }
                        }

                        if is_temporal {
                            args.push(format!("to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"", t, c, c));
                        } else {
                            args.push(format!("\"{}\".\"{}\"", t, c));
                        }
                    }
                }
                return args;
            }
            
            return self.select_columns.clone();
        }
        let mut flat_selects = Vec::new();
        for s in &self.select_columns {
            for sub in s.split(',') { flat_selects.push(sub.trim().to_string()); }
        }
        let mut expanded_tables = HashSet::new();
        for s in &flat_selects {
            if s == "*" { expanded_tables.insert(table_id.clone()); expanded_tables.insert(main_table_snake.clone()); }
            else if let Some(t) = s.strip_suffix(".*") { let t_clean = t.trim().trim_matches('"'); expanded_tables.insert(t_clean.to_string()); expanded_tables.insert(t_clean.to_snake_case()); }
        }
        let mut col_counts = HashMap::new();
        for col_info in &struct_cols {
            let col_snake = col_info.column.strip_prefix("r#").unwrap_or(col_info.column).to_snake_case();
            *col_counts.entry(col_snake).or_insert(0) += 1;
        }
        let is_tuple = format!("{:?}", std::any::type_name::<R>()).contains('(');
        let mut matched_s_indices = HashSet::new();
        let mut manual_field_map = HashMap::new();

        for (f_idx, s) in flat_selects.iter().enumerate() {
            if s == "*" || s.ends_with(".*") { continue; }
            let s_lower = s.to_lowercase();
            for (s_idx, col_info) in struct_cols.iter().enumerate() {
                if matched_s_indices.contains(&s_idx) { continue; }
                let col_snake = col_info.column.strip_prefix("r#").unwrap_or(col_info.column).to_snake_case();
                let mut m = false;
                if let Some((_, alias)) = s_lower.split_once(" as ") {
                    let ca = alias.trim().trim_matches('"').trim_matches('\'');
                    if ca == col_info.column || ca == &col_snake { m = true; }
                } else if s == col_info.column || s == &col_snake || s.ends_with(&format!(".{}", col_info.column)) || s.ends_with(&format!(".{}", col_snake)) {
                    m = true;
                }
                if m { manual_field_map.insert(f_idx, s_idx); matched_s_indices.insert(s_idx); break; }
            }
        }

        let mut args = Vec::new();
        if self.select_columns.is_empty() {
            for (s_idx, col_info) in struct_cols.iter().enumerate() {
                let mut t_use = table_id.clone();
                if !col_info.table.is_empty() {
                    let c_snake = col_info.table.to_snake_case();
                    if c_snake == main_table_snake { t_use = table_id.clone(); }
                    else if let Some(alias) = self.join_aliases.get(&c_snake) { t_use = alias.clone(); }
                    else if self.join_aliases.values().any(|a| a == &col_info.table) { t_use = col_info.table.to_string(); }
                }
                args.push(self.format_select_field::<R>(s_idx, &t_use, &main_table_snake, &col_counts, is_tuple));
            }
        } else {
            for (f_idx, s) in flat_selects.iter().enumerate() {
                let s_trim = s.trim();
                if s_trim == "*" || s_trim.ends_with(".*") {
                    let mut t_exp = if s_trim == "*" { String::new() } else { s_trim.strip_suffix(".*").unwrap_or(s_trim).trim().trim_matches('"').to_string() };
                    if !t_exp.is_empty() && (t_exp.to_snake_case() == main_table_snake || t_exp == table_id) { t_exp = table_id.clone(); }
                    for (s_idx, col_info) in struct_cols.iter().enumerate() {
                        if matched_s_indices.contains(&s_idx) { continue; }
                        let mut t_col = table_id.clone(); let mut known = false;
                        if !col_info.table.is_empty() {
                            let c_snake = col_info.table.to_snake_case();
                            if c_snake == main_table_snake { t_col = table_id.clone(); known = true; }
                            else if let Some(alias) = self.join_aliases.get(&c_snake) { t_col = alias.clone(); known = true; }
                            else if self.join_aliases.values().any(|a| a == &col_info.table) { t_col = col_info.table.to_string(); known = true; }
                        }
                        if !known && !t_exp.is_empty() && flat_selects.iter().filter(|x| x.ends_with(".*") || *x == "*").count() == 1 { t_col = t_exp.clone(); known = true; }
                        if (t_exp.is_empty() && known) || (!t_exp.is_empty() && t_col == t_exp) {
                            args.push(self.format_select_field::<R>(s_idx, &t_col, &main_table_snake, &col_counts, is_tuple));
                            matched_s_indices.insert(s_idx);
                        }
                    }
                } else if let Some(s_idx) = manual_field_map.get(&f_idx) {
                    if s.to_lowercase().contains(" as ") { args.push(s_trim.to_string()); }
                    else {
                        let mut t = table_id.clone();
                        if let Some((prefix, _)) = s_trim.split_once('.') { t = prefix.trim().trim_matches('"').to_string(); }
                        args.push(self.format_select_field::<R>(*s_idx, &t, &main_table_snake, &col_counts, is_tuple));
                    }
                } else {
                    if !s_trim.contains(' ') && !s_trim.contains('(') {
                        if let Some((t, c)) = s_trim.split_once('.') { args.push(format!("\"{}\".\"{}\"", t.trim().trim_matches('"'), c.trim().trim_matches('"'))); }
                        else { args.push(format!("\"{}\"", s_trim.trim_matches('"'))); }
                    } else { args.push(s_trim.to_string()); }
                }
            }
        }
        if args.is_empty() { vec!["*".to_string()] } else { args }
    }

    fn format_select_field<R: AnyImpl>(&self, s_idx: usize, table_to_use: &str, main_table_snake: &str, col_counts: &HashMap<String, usize>, is_tuple: bool) -> String {
        let col_info = &R::columns()[s_idx];
        let col_snake = col_info.column.strip_prefix("r#").unwrap_or(col_info.column).to_snake_case();
        let has_collision = *col_counts.get(&col_snake).unwrap_or(&0) > 1;
        let alias = if is_tuple || has_collision {
            let t_alias = if !col_info.table.is_empty() { col_info.table.to_snake_case() } else { main_table_snake.to_string() };
            format!("{}__{}", t_alias.to_lowercase(), col_snake.to_lowercase())
        } else { col_snake.to_lowercase() };
        if is_temporal_type(col_info.sql_type) && matches!(self.driver, Drivers::Postgres) {
            format!("to_json(\"{}\".\"{}\") #>> '{{}}' AS \"{}\"", table_to_use, col_snake, alias)
        } else {
            format!("\"{}\".\"{}\" AS \"{}\"", table_to_use, col_snake, alias)
        }
    }

    /// Executes the query and returns a list of results.
    ///
    /// This method builds and executes a SELECT query with all accumulated filters,
    /// ordering, and pagination settings. It returns all matching rows as a vector.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The result type. Must implement `FromAnyRow` and `AnyImpl`.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<R>)` - Vector of results (empty if no matches)
    /// * `Err(sqlx::Error)` - Database error during query execution
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = db.model::<User>()
    ///     .filter("age", Op::Gte, 18)
    ///     .scan()
    ///     .await?;
    /// // SQL: SELECT * FROM "user" WHERE "age" >= 18
    /// ```
    pub async fn scan<R>(mut self) -> Result<Vec<R>, sqlx::Error>
    where
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        self.apply_soft_delete_filter();
        let mut query = String::new();
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        self.write_select_sql::<R>(&mut query, &mut args, &mut arg_counter);

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let rows = self.tx.fetch_all(&query, args).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(R::from_any_row(&row)?);
        }
        Ok(result)
    }

    /// Executes the query and eager loads the requested relationships.
    pub async fn scan_with(self) -> Result<Vec<T>, sqlx::Error>
    where
        T: FromAnyRow + AnyImpl + crate::model::Model + Send + Unpin + 'static,
        E: Connection + Clone,
    {
        self.scan_as_with::<T>().await
    }

    /// Executes the query, maps the result to a DTO, and eager loads relationships for the DTO.
    /// 
    /// This is useful when you want to return a different struct than the Model,
    /// but still want to take advantage of the Eager Loading system.
    /// The DTO must implement the `Model` trait (can be derived with #[orm(table = "...")])
    pub async fn scan_as_with<R>(mut self) -> Result<Vec<R>, sqlx::Error>
    where
        R: FromAnyRow + AnyImpl + crate::model::Model + Send + Unpin + 'static,
        E: Clone,
    {
        let with_relations = std::mem::take(&mut self.with_relations);
        let with_modifiers = std::mem::take(&mut self.with_modifiers);
        let tx = self.tx.clone();
        
        // Execute the main query and map to R
        let mut results: Vec<R> = self.scan_as::<R>().await?;

        if !results.is_empty() && !with_relations.is_empty() {
            let mut grouped: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            for rel in with_relations {
                if let Some(pos) = rel.find('.') {
                    let base = rel[..pos].to_string();
                    let nested = rel[pos + 1..].to_string();
                    grouped.entry(base).or_default().push(nested);
                } else {
                    grouped.entry(rel).or_default();
                }
            }

            for (base, nested_parts) in grouped {
                let modifier = with_modifiers.get(&base).cloned();
                
                let full_rel = if nested_parts.is_empty() {
                    base
                } else {
                    let filtered: Vec<_> = nested_parts.into_iter().filter(|s| !s.is_empty()).collect();
                    if filtered.is_empty() {
                        base
                    } else if filtered.len() == 1 {
                        format!("{}.{}", base, filtered[0])
                    } else {
                        format!("{}.({})", base, filtered.join("|"))
                    }
                };
                R::load_relations(&full_rel, &mut results, &tx, modifier).await?;
            }
        }

        Ok(results)
    }

    /// Executes the query and maps the result to a custom DTO.
    ///
    /// Useful for queries that return only a subset of columns or join multiple tables.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The DTO type. Must implement `FromAnyRow` and `AnyImpl`.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<R>)` - Vector of results
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let dtos: Vec<UserStats> = db.model::<User>()
    ///     .select("username, age")
    ///     .scan_as::<UserStats>()
    ///     .await?;
    /// // SQL: SELECT "username", "age" FROM "user"
    /// ```
    pub async fn scan_as<R>(mut self) -> Result<Vec<R>, sqlx::Error>
    where
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        self.apply_soft_delete_filter();
        let mut query = String::new();
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        self.write_select_sql::<R>(&mut query, &mut args, &mut arg_counter);

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let rows = self.tx.fetch_all(&query, args).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(R::from_any_row(&row)?);
        }
        Ok(result)
    }

    /// Executes the query and returns only the first result.
    ///
    /// Automatically applies `LIMIT 1` if no limit is set.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The result type. Must implement `FromAnyRow` and `AnyImpl`.
    ///
    /// # Returns
    ///
    /// * `Ok(R)` - The first matching record
    /// * `Err(sqlx::Error::RowNotFound)` - If no records match
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .first()
    ///     .await?;
    /// // SQL: SELECT * FROM "user" WHERE "id" = 1 LIMIT 1
    /// ```
    pub async fn first<R>(mut self) -> Result<R, sqlx::Error>
    where
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        self.apply_soft_delete_filter();
        let mut query = String::new();
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        // Force limit 1 if not set
        if self.limit.is_none() {
            self.limit = Some(1);
        }

        // Apply PK ordering fallback if no order is set
        if self.order_clauses.is_empty() {
            let table_id = self.get_table_identifier();
            let pk_columns: Vec<String> = <T as Model>::columns()
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| format!("\"{}\".\"{}\"", table_id, c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case()))
                .collect();
            
            if !pk_columns.is_empty() {
                self.order_clauses.push(pk_columns.iter().map(|col| format!("{} ASC", col)).collect::<Vec<_>>().join(", "));
            }
        }

        self.write_select_sql::<R>(&mut query, &mut args, &mut arg_counter);

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let row = self.tx.fetch_one(&query, args).await?;
        R::from_any_row(&row)
    }

    /// Executes the query and returns a single scalar value.
    ///
    /// This method is useful for fetching single values like counts, max/min values,
    /// or specific columns without mapping to a struct or tuple.
    ///
    /// # Type Parameters
    ///
    /// * `O` - The output type. Must implement `FromAnyRow`, `AnyImpl`, `Send` and `Unpin`.
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
        O: FromAnyRow + AnyImpl + Send + Unpin,
    {
        self.apply_soft_delete_filter();
        let mut query = String::new();
        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        // Force limit 1 if not set
        if self.limit.is_none() {
            self.limit = Some(1);
        }

        self.write_select_sql::<O>(&mut query, &mut args, &mut arg_counter);

        if self.debug_mode {
            log::debug!("SQL: {}", query);
        }

        let row = self.tx.fetch_one(&query, args).await?;
        O::from_any_row(&row)
    }

    /// Updates a single column in the database for all rows matching the filters.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to update
    /// * `value` - The new value (supports primitive types and Option for NULL)
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .update("active", false)
    ///     .await?;
    /// ```
    pub fn update<'b, V>(&'b mut self, col: &str, value: V) -> BoxFuture<'b, Result<u64, sqlx::Error>>
    where
        V: ToUpdateValue + Send + Sync,
    {
        let mut map = std::collections::HashMap::new();
        map.insert(col.to_string(), value.to_update_value());
        self.execute_update(map)
    }

    /// Updates columns based on a model instance for all rows matching the filters.
    ///
    /// This method updates the table with values from the provided model.
    /// Note: It updates ALL columns present in the model's `to_map()`.
    ///
    /// # Arguments
    ///
    /// * `model` - The model instance containing new values
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows affected
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user = User { id: 1, username: "new_name".to_string(), ... };
    /// db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .updates(&user)
    ///     .await?;
    /// ```
    pub fn updates<'b>(&'b mut self, model: &T) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        self.execute_update(Model::to_map(model))
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(AnyRow)]
    /// struct UserUpdate {
    ///     username: String,
    ///     active: bool,
    /// }
    ///
    /// let partial = UserUpdate { username: "updated".into(), active: true };
    /// db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .update_partial(&partial)
    ///     .await?;
    /// ```
    pub fn update_partial<'b, P: AnyImpl>(&'b mut self, partial: &P) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        self.execute_update(AnyImpl::to_map(partial))
    }

    /// Updates a column using a raw SQL expression.
    ///
    /// This allows for complex updates like incrementing values or using database functions.
    /// You can use a `?` placeholder in the expression and provide a value to bind.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name to update
    /// * `expr` - The raw SQL expression (e.g., "age + 1" or "age + ?")
    /// * `value` - The value to bind for the placeholder
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Increment age by 1
    /// db.model::<User>()
    ///     .filter("id", "=", 1)
    ///     .update_raw("age", "age + 1", 0)
    ///     .await?;
    ///
    /// // Increment age by a variable
    /// db.model::<User>()
    ///     .filter("id", "=", 1)
    ///     .update_raw("age", "age + ?", 5)
    ///     .await?;
    /// ```
    pub fn update_raw<'b, V>(
        &'b mut self,
        col: &str,
        expr: &str,
        value: V,
    ) -> BoxFuture<'b, Result<u64, sqlx::Error>>
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone,
    {
        self.apply_soft_delete_filter();

        let col_name_clean = col.strip_prefix("r#").unwrap_or(col).to_snake_case();
        let expr_owned = expr.to_string();
        let value_owned = value.clone();

        Box::pin(async move {
            let table_name = self.table_name.to_snake_case();
            let mut query = format!("UPDATE \"{}\" ", table_name);
            if let Some(alias) = &self.alias {
                query.push_str(&format!("AS {} ", alias));
            }
            query.push_str("SET ");

            let mut arg_counter = 1;
            let mut args = AnyArguments::default();

            let mut processed_expr = expr_owned.clone();
            let mut has_placeholder = false;

            if processed_expr.contains('?') {
                has_placeholder = true;
                if matches!(self.driver, Drivers::Postgres) {
                    while let Some(pos) = processed_expr.find('?') {
                        let placeholder = format!("${}", arg_counter);
                        arg_counter += 1;
                        processed_expr.replace_range(pos..pos + 1, &placeholder);
                    }
                }
            }

            if has_placeholder {
                let _ = args.add(value_owned);
            }

            query.push_str(&format!("\"{}\" = {}", col_name_clean, processed_expr));
            query.push_str(" WHERE 1=1");

            for clause in &self.where_clauses {
                clause(&mut query, &mut args, &self.driver, &mut arg_counter);
            }

            if self.debug_mode {
                log::debug!("SQL: {}", query);
            }

            let result = self.tx.execute(&query, args).await?;
            Ok(result.rows_affected())
        })
    }

    /// Internal helper to apply soft delete filter to where clauses if necessary.
    fn apply_soft_delete_filter(&mut self) {
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
    }

    /// Internal helper to execute an UPDATE query from a map of values.
    fn execute_update<'b>(
        &'b mut self,
        data_map: std::collections::HashMap<String, Option<String>>,
    ) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        self.apply_soft_delete_filter();

        Box::pin(async move {
            let table_name = self.table_name.to_snake_case();
            let mut query = format!("UPDATE \"{}\" ", table_name);
            if let Some(alias) = &self.alias {
                query.push_str(&format!("{} ", alias));
            }
            query.push_str("SET ");

            let mut bindings: Vec<(Option<String>, &str)> = Vec::new();
            let mut set_clauses = Vec::new();

            // Maintain argument counter for PostgreSQL ($1, $2, ...)
            let mut arg_counter = 1;

            // Build SET clause
            for (col_name, value) in data_map {
                // Strip the "r#" prefix if present
                let col_name_clean = col_name.strip_prefix("r#").unwrap_or(&col_name).to_snake_case();

                // Find the SQL type for this column from the Model metadata
                let sql_type_opt = self
                    .columns_info
                    .iter()
                    .find(|c| c.name == col_name || c.name == col_name_clean)
                    .map(|c| c.sql_type);
                    
                let sql_type = match sql_type_opt {
                    Some(t) => t,
                    None => continue,
                };

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
                                s if s.ends_with("[]") => format!("${}::{}", idx, s),
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
            for (val_opt, sql_type) in bindings {
                if let Some(val_str) = val_opt {
                    if args.bind_value(&val_str, sql_type, &self.driver).is_err() {
                        let _ = args.add(val_str);
                    }
                } else {
                    let _ = args.add(None::<String>);
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
            let result = self.tx.execute(&query, args).await?;

            Ok(result.rows_affected())
        })
    }

    /// Executes a DELETE query based on the current filters.
    ///
    /// Performs a soft delete if the model has a soft delete column,
    /// otherwise performs a permanent hard delete.
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows deleted (or soft-deleted)
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .delete()
    ///     .await?;
    /// // SQL (Soft): UPDATE "user" SET "deleted_at" = NOW() WHERE "id" = 1
    /// // SQL (Hard): DELETE FROM "user" WHERE "id" = 1
    /// ```
    pub async fn delete(self) -> Result<u64, sqlx::Error> {
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

            let result = self.tx.execute(&query, args).await?;
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

            let result = self.tx.execute(&query, args).await?;
            Ok(result.rows_affected())
        }
    }

    /// Permanently removes records from the database.
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - The number of rows deleted
    /// * `Err(sqlx::Error)` - Database error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>()
    ///     .filter("id", Op::Eq, 1)
    ///     .hard_delete()
    ///     .await?;
    /// // SQL: DELETE FROM "user" WHERE "id" = 1
    /// ```
    pub async fn hard_delete(self) -> Result<u64, sqlx::Error> {
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

        let result = self.tx.execute(&query, args).await?;
        Ok(result.rows_affected())
    }
}
