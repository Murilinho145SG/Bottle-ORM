//! # Pagination Module
//!
//! This module provides pagination functionality for Bottle ORM queries.
//! It handles the calculation of limits, offsets, and total page counts,
//! and integrates seamlessly with the `QueryBuilder`.

// ============================================================================
// External Crate Imports
// ============================================================================

use serde::{Deserialize, Serialize};
use sqlx::Row;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{
    any_struct::FromAnyRow,
    database::Connection,
    model::Model,
    query_builder::QueryBuilder,
    AnyImpl,
};

// ============================================================================
// Pagination Structs
// ============================================================================

/// Represents a paginated result set from the database.
///
/// Contains the requested subset of data along with metadata about the total
/// number of records and pages available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// The list of items for the current page
    pub data: Vec<T>,
    /// The total number of records matching the query (ignoring pagination)
    pub total: i64,
    /// The current page number (zero-based)
    pub page: usize,
    /// The number of items per page
    pub limit: usize,
    /// The total number of pages available
    pub total_pages: i64,
}

/// A builder for pagination settings.
///
/// Use this struct to define how results should be paginated before executing
/// a query via `paginate()`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    /// Zero-based page index
    pub page: usize,
    /// Number of items per page
    pub limit: usize,
    /// Maximum allowed items per page (safety limit)
    pub max_limit: usize,
}

impl Pagination {
    /// Creates a new Pagination instance with a custom safety limit.
    ///
    /// # Arguments
    ///
    /// * `page` - Zero-based page number
    /// * `limit` - Items per page
    /// * `max_limit` - Maximum allowed items per page
    pub fn new_with_limit(page: usize, limit: usize, max_limit: usize) -> Self {
        let mut f_limit = limit;
        if f_limit > max_limit {
            f_limit = 10;
        }
        Self { page, limit: f_limit, max_limit }
    }

    /// Creates a new Pagination instance with a default safety limit of 100.
    ///
    /// # Arguments
    ///
    /// * `page` - Zero-based page number
    /// * `limit` - Items per page
    pub fn new(page: usize, limit: usize) -> Self {
        Self::new_with_limit(page, limit, 100)
    }

    /// Applies pagination settings to a `QueryBuilder`.
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
    pub fn apply<T, E>(mut self, query: QueryBuilder<T, E>) -> QueryBuilder<T, E>
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

    /// Executes the query and returns a `Paginated<R>` structure.
    ///
    /// This method performs two database operations:
    /// 1. A `COUNT(*)` query to determine total records.
    /// 2. The actual data query with `LIMIT` and `OFFSET` applied.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The base Model type for the query.
    /// * `E` - The connection type.
    /// * `R` - The target result type (usually the same as T or a DTO).
    ///
    /// # Returns
    ///
    /// * `Ok(Paginated<R>)` - The data and pagination metadata.
    /// * `Err(sqlx::Error)` - Database error.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let p = Pagination::new(0, 20);
    /// let res: Paginated<User> = p.paginate(db.model::<User>()).await?;
    ///
    /// for user in res.data {
    ///     println!("User: {}", user.username);
    /// }
    /// ```
    pub async fn paginate<T, E, R>(self, mut query: QueryBuilder<T, E>) -> Result<Paginated<R>, sqlx::Error>
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
        let count_row = query.tx.fetch_one(&count_sql, args).await?;

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
    
    /// Executes the query and returns a `Paginated<R>` mapping to a custom DTO.
    ///
    /// This method is similar to `paginate`, but it uses `scan_as` to map the results
    /// to a type `R` that implements `FromAnyRow` but does not necessarily implement `AnyImpl`.
    /// This is particularly useful for complex queries involving JOINs where the result
    /// doesn't map directly to a single `Model`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The base Model type for the query.
    /// * `E` - The connection type.
    /// * `R` - The target result type (DTO/Projection).
    ///
    /// # Returns
    ///
    /// * `Ok(Paginated<R>)` - The paginated results mapped to type `R`.
    /// * `Err(sqlx::Error)` - Database error.
    pub async fn paginate_as<T, E, R>(self, mut query: QueryBuilder<T, E>) -> Result<Paginated<R>, sqlx::Error>
    where
        T: Model + Send + Sync + Unpin,
        E: Connection + Send,
        R: FromAnyRow + AnyImpl + Send + Unpin,
    {
        // 1. Prepare COUNT query
        let original_select = query.select_columns.clone();
        let original_order = query.order_clauses.clone();
        let _original_limit = query.limit;
        let _original_offset = query.offset;
    
        query.select_columns = vec!["COUNT(*)".to_string()];
        query.order_clauses.clear();
        query.limit = None;
        query.offset = None;
    
        let count_sql = query.to_sql();
    
        let mut args = sqlx::any::AnyArguments::default();
        let mut arg_counter = 1;
    
        let mut dummy_query = String::new();
        for clause in &query.where_clauses {
            clause(&mut dummy_query, &mut args, &query.driver, &mut arg_counter);
        }
        if !query.having_clauses.is_empty() {
            for clause in &query.having_clauses {
                clause(&mut dummy_query, &mut args, &query.driver, &mut arg_counter);
            }
        }
    
        let count_row = query.tx.fetch_one(&count_sql, args).await?;
        let total: i64 = count_row.try_get(0)?;
    
        // 3. Restore Query State
        query.select_columns = original_select;
        query.order_clauses = original_order;
        query.limit = Some(self.limit);
        query.offset = Some(self.page * self.limit);
    
        // 4. Execute Data Query usando o novo SCAN_AS
        let data = query.scan_as::<R>().await?;
    
        // 5. Calculate Metadata
        let total_pages = (total as f64 / self.limit as f64).ceil() as i64;
    
        Ok(Paginated { data, total, page: self.page, limit: self.limit, total_pages })
    }
}
