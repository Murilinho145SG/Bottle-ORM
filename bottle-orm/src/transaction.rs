//! # Transaction Module
//!
//! This module provides the transaction management functionality for Bottle ORM.
//! It allows executing multiple database operations atomically, ensuring data consistency.
//!
//! ## Features
//!
//! - **Atomic Operations**: Group multiple queries into a single unit of work
//! - **Automatic Rollback**: Transactions are automatically rolled back if dropped without commit
//! - **Driver Agnostic**: Works consistently across PostgreSQL, MySQL, and SQLite
//! - **Fluent API**: Integrated with `QueryBuilder` for seamless usage
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bottle_orm::Database;
//!
//! let mut tx = db.begin().await?;
//!
//! // Operations within transaction
//! tx.model::<User>().insert(&user).await?;
//! tx.model::<Post>().insert(&post).await?;
//!
//! // Commit changes
//! tx.commit().await?;
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use heck::ToSnakeCase;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{
    database::{Connection, Drivers, RawQuery},
    Model, QueryBuilder,
};

// ============================================================================
// Transaction Struct
// ============================================================================

/// A wrapper around a SQLx transaction.
///
/// Provides a way to execute multiple queries atomically. If any query fails,
/// the transaction can be rolled back. If all succeed, it can be committed.
///
/// # Type Parameters
///
/// * `'a` - The lifetime of the database connection source
///
/// # Fields
///
/// * `tx` - The underlying SQLx transaction
/// * `driver` - The database driver type (for query syntax handling)
#[derive(Debug)]
pub struct Transaction<'a> {
    pub(crate) tx: sqlx::Transaction<'a, sqlx::Any>,
    pub(crate) driver: Drivers,
}

// ============================================================================
// Connection Implementation
// ============================================================================

/// Implementation of Connection for a Transaction.
///
/// Allows the `QueryBuilder` to use a transaction for executing queries.
/// Supports generic borrow lifetimes to allow multiple operations within
/// the same transaction scope.
impl<'a> Connection for Transaction<'a> {
    type Exec<'c>
        = &'c mut sqlx::AnyConnection
    where
        Self: 'c;

    fn executor<'c>(&'c mut self) -> Self::Exec<'c> {
        &mut *self.tx
    }
}

/// Implementation of Connection for a mutable reference to Transaction.
impl<'a, 'b> Connection for &'a mut Transaction<'b> {
    type Exec<'c>
        = &'c mut sqlx::AnyConnection
    where
        Self: 'c;

    fn executor<'c>(&'c mut self) -> Self::Exec<'c> {
        (**self).executor()
    }
}

// ============================================================================
// Transaction Implementation
// ============================================================================

impl<'a> Transaction<'a> {
    // ========================================================================
    // Query Building
    // ========================================================================

    /// Starts building a query within this transaction.
    ///
    /// This method creates a new `QueryBuilder` that will execute its queries
    /// as part of this transaction.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type to query.
    ///
    /// # Returns
    ///
    /// A new `QueryBuilder` instance bound to this transaction.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut tx = db.begin().await?;
    ///
    /// // These operations are part of the transaction
    /// tx.model::<User>().insert(&user).await?;
    /// tx.model::<Post>().insert(&post).await?;
    ///
    /// tx.commit().await?;
    /// ```
    pub fn model<T: Model + Send + Sync + Unpin>(
        &mut self,
    ) -> QueryBuilder<'a, T, &mut sqlx::Transaction<'a, sqlx::Any>> {
        // Get active column names from the model
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());

        // Convert column names to snake_case and strip 'r#' prefix if present
        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        // Create and return the query builder
        QueryBuilder::new(&mut self.tx, self.driver, T::table_name(), T::columns(), columns)
    }

    /// Creates a raw SQL query builder attached to this transaction.
    ///
    /// Allows executing raw SQL queries that participate in the current transaction.
    ///
    /// # Arguments
    ///
    /// * `sql` - The raw SQL query string
    ///
    /// # Returns
    ///
    /// A `RawQuery` builder bound to this transaction.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut tx = db.begin().await?;
    ///
    /// // Execute raw SQL inside transaction
    /// tx.raw("INSERT INTO logs (msg) VALUES ($1)")
    ///     .bind("Transaction started")
    ///     .execute()
    ///     .await?;
    ///
    /// tx.commit().await?;
    /// ```
    pub fn raw<'b>(&'b mut self, sql: &'b str) -> RawQuery<'b, &'b mut Transaction<'a>> {
        RawQuery::new(self, sql)
    }

    // ========================================================================
    // Transaction Control
    // ========================================================================

    /// Commits the transaction.
    ///
    /// Persists all changes made during the transaction to the database.
    /// This consumes the `Transaction` instance.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transaction committed successfully
    /// * `Err(sqlx::Error)` - Database error during commit
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.tx.commit().await
    }

    /// Rolls back the transaction.
    ///
    /// Reverts all changes made during the transaction. This happens automatically
    /// if the `Transaction` is dropped without being committed, but this method
    /// allows for explicit rollback.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transaction rolled back successfully
    /// * `Err(sqlx::Error)` - Database error during rollback
    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        self.tx.rollback().await
    }
}
