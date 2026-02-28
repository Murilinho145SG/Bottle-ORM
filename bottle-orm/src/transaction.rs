//! # Transaction Module
//!
//! This module provides the transaction management functionality for Bottle ORM.
//! It allows executing multiple database operations atomically, ensuring data consistency.

// ============================================================================
// External Crate Imports
// ============================================================================

use heck::ToSnakeCase;
use std::sync::Arc;
use tokio::sync::Mutex;
use futures::future::BoxFuture;
use sqlx::any::AnyArguments;

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
#[derive(Debug, Clone)]
pub struct Transaction<'a> {
    pub(crate) tx: Arc<Mutex<Option<sqlx::Transaction<'a, sqlx::Any>>>>,
    pub(crate) driver: Drivers,
}

// Transaction is Send and Sync because it uses Arc<Mutex>.
// This allows it to be used easily in async handlers (like Axum).

// ============================================================================
// Connection Implementation
// ============================================================================

impl Connection for Transaction<'_> {
    fn execute<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyQueryResult, sqlx::Error>> {
        Box::pin(async move {
            let mut guard = self.tx.lock().await;
            if let Some(tx) = guard.as_mut() {
                sqlx::query_with(sql, args).execute(&mut **tx).await
            } else {
                Err(sqlx::Error::WorkerCrashed)
            }
        })
    }

    fn fetch_all<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Vec<sqlx::any::AnyRow>, sqlx::Error>> {
        Box::pin(async move {
            let mut guard = self.tx.lock().await;
            if let Some(tx) = guard.as_mut() {
                sqlx::query_with(sql, args).fetch_all(&mut **tx).await
            } else {
                Err(sqlx::Error::WorkerCrashed)
            }
        })
    }

    fn fetch_one<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyRow, sqlx::Error>> {
        Box::pin(async move {
            let mut guard = self.tx.lock().await;
            if let Some(tx) = guard.as_mut() {
                sqlx::query_with(sql, args).fetch_one(&mut **tx).await
            } else {
                Err(sqlx::Error::WorkerCrashed)
            }
        })
    }

    fn fetch_optional<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Option<sqlx::any::AnyRow>, sqlx::Error>> {
        Box::pin(async move {
            let mut guard = self.tx.lock().await;
            if let Some(tx) = guard.as_mut() {
                sqlx::query_with(sql, args).fetch_optional(&mut **tx).await
            } else {
                Err(sqlx::Error::WorkerCrashed)
            }
        })
    }
}

// ============================================================================
// Transaction Implementation
// ============================================================================

impl<'a> Transaction<'a> {
    /// Starts building a query within this transaction.
    pub fn model<T: Model + Send + Sync + Unpin>(
        &self,
    ) -> QueryBuilder<T, Self> {
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());

        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        QueryBuilder::new(self.clone(), self.driver, T::table_name(), T::columns(), columns)
    }

    /// Creates a raw SQL query builder attached to this transaction.
    pub fn raw<'b>(&self, sql: &'b str) -> RawQuery<'b, Self> {
        RawQuery::new(self.clone(), sql)
    }

    /// Commits the transaction.
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        let mut guard = self.tx.lock().await;
        if let Some(tx) = guard.take() {
            tx.commit().await
        } else {
            Ok(())
        }
    }

    /// Rolls back the transaction.
    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        let mut guard = self.tx.lock().await;
        if let Some(tx) = guard.take() {
            tx.rollback().await
        } else {
            Ok(())
        }
    }
}
