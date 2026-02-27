//! # Pagination Module
//!
//! This module provides a standard `Pagination` struct that is compatible with
//! web frameworks like `axum`, `actix-web`, and `serde`. It allows for easy
//! extraction of pagination parameters from HTTP requests and application
//! to `QueryBuilder` instances.
//!
//! ## Features
//!
//! - **Serde Compatibility**: derives `Serialize` and `Deserialize`
//! - **Query Integration**: `apply` method to automatically paginate queries
//! - **Defaults**: sane defaults (page 0, limit 10)
//!
//! ## Example with Axum
//!
//! ```rust,ignore
//! use axum::{extract::Query, Json};
//! use bottle_orm::{Database, pagination::Pagination};
//!
//! async fn list_users(
//!     State(db): State<Database>,
//!     Query(pagination): Query<Pagination>
//! ) -> Json<Vec<User>> {
//!     let users = pagination.apply(db.model::<User>())
//!         .scan()
//!         .await
//!         .unwrap();
//!
//!     Json(users)
//! }
//! ```

use crate::{AnyImpl, any_struct::FromAnyRow, database::Connection, model::Model, query_builder::QueryBuilder};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// A standard pagination structure.
///
/// Can be deserialized from query parameters (e.g., `?page=1&limit=20`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    /// The page number (0-indexed). Default: 0.
    #[serde(default)]
    pub page: usize,

    /// The number of items per page. Default: 10.
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// The maximum allowed limit for pagination to prevent large result sets.
    /// If the requested `limit` exceeds `max_limit`, it will be capped (default: 100).
    #[serde(default = "default_max_limit", skip_deserializing)]
    pub max_limit: usize,
}

/// A wrapper for paginated results.
///
/// Contains the data items and metadata about the pagination state (total, pages, etc.).
/// This struct is `Serialize`d to JSON for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// The list of items for the current page.
    pub data: Vec<T>,
    /// The total number of items matching the query.
    pub total: i64,
    /// The current page number (0-indexed).
    pub page: usize,
    /// The number of items per page.
    pub limit: usize,
    /// The total number of pages.
    pub total_pages: i64,
}

fn default_limit() -> usize {
    10
}

fn default_max_limit() -> usize {
	100
}

impl Default for Pagination {
    fn default() -> Self {
        Self { page: 0, limit: 10, max_limit: 100 }
    }
}

impl Pagination {
    /// Creates a new Pagination instance with a specified max_limit.
    ///
    /// If `limit` is greater than `max_limit`, it defaults to 10.
    pub fn new(page: usize, mut limit: usize, max_limit: usize) -> Self {
    	if limit > max_limit {
     		limit = 10;
     	}
        Self { page, limit, max_limit }
    }

    /// Applies the pagination to a `QueryBuilder`.
    ///
    /// This method sets the `limit` and `offset` of the query builder
    /// based on the pagination parameters. It also enforces the `max_limit`
    /// check before applying the limit.
    ///
    /// # Arguments
    ///
    /// * `query` - The `QueryBuilder` to paginate
    ///
    /// # Returns
    ///
    /// The modified `QueryBuilder`
    pub fn apply<'a, T, E>(mut self, query: QueryBuilder<'a, T, E>) -> QueryBuilder<'a, T, E>
    where
        T: Model + Send + Sync + Unpin,
        E: Connection + Send,
    {
        // Enforce max_limit again during application to ensure safety
        if self.limit > self.max_limit {
        	self.limit = 10;
        }
        query.limit(self.limit).offset(self.page * self.limit)
    }

    /// Executes the query and returns a `Paginated<T>` result with metadata.
    ///
    /// This method performs two database queries:
    /// 1. A `COUNT(*)` query to get the total number of records matching the filters.
    /// 2. The actual `SELECT` query with `LIMIT` and `OFFSET` applied.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type.
    /// * `E` - The connection type (Database or Transaction).
    /// * `R` - The result type (usually same as T, but can be a DTO/Projection).
    ///
    /// # Returns
    ///
    /// * `Ok(Paginated<R>)` - The paginated results.
    /// * `Err(sqlx::Error)` - Database error.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let pagination = Pagination::new(0, 10);
    /// let result = pagination.paginate(db.model::<User>()).await?;
    ///
    /// println!("Total users: {}", result.total);
    /// for user in result.data {
    ///     println!("User: {}", user.username);
    /// }
    /// ```
    pub async fn paginate<'a, T, E, R>(self, mut query: QueryBuilder<'a, T, E>) -> Result<Paginated<R>, sqlx::Error>
    where
        T: Model + Send + Sync + Unpin,
        E: Connection + Send,
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        // 1. Prepare COUNT query
        // We temporarily replace selected columns with COUNT(*) and remove order/limit/offset
        let original_select = query.select_columns.clone();
        let original_order = query.order_clauses.clone();
        let _original_limit = query.limit;
        let _original_offset = query.offset;

        query.select_columns = vec!["COUNT(*)".to_string()];
        query.order_clauses.clear();
        query.limit = None;
        query.offset = None;

        // 2. Generate and Execute Count SQL
        // We cannot use query.scalar() easily because it consumes self.
        // We use query.to_sql() and construct a manual query execution using the builder's state.

        let count_sql = query.to_sql();

        // We need to re-bind arguments. This logic mirrors QueryBuilder::scan
        let mut args = sqlx::any::AnyArguments::default();
        let mut arg_counter = 1;

        // Re-bind arguments for count query
        // Note: We access internal fields of QueryBuilder. This assumes this module is part of the crate.
        // If WHERE clauses are complex, this manual reconstruction is necessary.
        let mut dummy_query = String::new(); // Just to satisfy the closure signature
        for clause in &query.where_clauses {
            clause(&mut dummy_query, &mut args, &query.driver, &mut arg_counter);
        }
        if !query.having_clauses.is_empty() {
            for clause in &query.having_clauses {
                clause(&mut dummy_query, &mut args, &query.driver, &mut arg_counter);
            }
        }

        // Execute count query
        let count_row = sqlx::query_with::<_, _>(&count_sql, args).fetch_one(query.tx.executor()).await?;

        let total: i64 = count_row.try_get(0)?;

        // 3. Restore Query State for Data Fetch
        query.select_columns = original_select;
        query.order_clauses = original_order;
        // Apply Pagination
        query.limit = Some(self.limit);
        query.offset = Some(self.page * self.limit);

        // 4. Execute Data Query
        // Now we can consume the builder with scan()
        let data = query.scan::<R>().await?;

        // 5. Calculate Metadata
        let total_pages = (total as f64 / self.limit as f64).ceil() as i64;

        Ok(Paginated { data, total, page: self.page, limit: self.limit, total_pages })
    }
}
