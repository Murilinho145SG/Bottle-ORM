//! # Database Module
//!
//! This module provides the core database connection and management functionality for Bottle ORM.
//! It handles connection pooling, driver detection, table creation, and foreign key management
//! across PostgreSQL, MySQL, and SQLite.

// ============================================================================
// External Crate Imports
// ============================================================================

use futures::future::BoxFuture;
use heck::ToSnakeCase;
use sqlx::{any::AnyArguments, AnyPool, Row, Arguments};
use std::sync::Arc;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{migration::Migrator, Error, Model, QueryBuilder};

// ============================================================================
// Database Driver Enum
// ============================================================================

/// Supported database drivers for Bottle ORM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drivers {
    /// PostgreSQL driver
    Postgres,
    /// MySQL driver
    MySQL,
    /// SQLite driver
    SQLite,
}

// ============================================================================
// Database Struct
// ============================================================================

/// The main entry point for Bottle ORM database operations.
///
/// `Database` manages a connection pool and provides methods for starting
/// transactions, creating tables, and building queries for models.
///
/// It is designed to be thread-safe and easily shared across an application
/// (internally uses an `Arc` for the connection pool).
#[derive(Debug, Clone)]
pub struct Database {
    /// The underlying SQLx connection pool
    pub(crate) pool: AnyPool,
    /// The detected database driver
    pub(crate) driver: Drivers,
}

// ============================================================================
// Database Implementation
// ============================================================================

impl Database {
    /// Creates a new DatabaseBuilder for configuring the connection.
    pub fn builder() -> DatabaseBuilder {
        DatabaseBuilder::new()
    }

    /// Connects to a database using the provided connection string.
    ///
    /// This is a convenience method that uses default builder settings.
    ///
    /// # Arguments
    ///
    /// * `url` - A database connection URL (e.g., "postgres://user:pass@localhost/db")
    pub async fn connect(url: &str) -> Result<Self, Error> {
        DatabaseBuilder::new().connect(url).await
    }

    /// Returns a new Migrator instance for managing schema changes.
    pub fn migrator(&self) -> Migrator<'_> {
        Migrator::new(self)
    }

    /// Starts building a query for the specified model.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type to query.
    pub fn model<T: Model + Send + Sync + Unpin + crate::AnyImpl>(&self) -> QueryBuilder<T, Self> {
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());

        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        QueryBuilder::new(self.clone(), self.driver, T::table_name(), <T as Model>::columns(), columns)
    }

    /// Creates a raw SQL query builder.
    pub fn raw<'a>(&self, sql: &'a str) -> RawQuery<'a, Self> {
        RawQuery::new(self.clone(), sql)
    }
    
    /// This function should have been here a long time ago.
    /// Retrieve the connection pool.
    pub fn get_pool(&self) -> AnyPool {
    	self.pool.clone()
    }

    /// Starts a new database transaction.
    pub async fn begin(&self) -> Result<crate::transaction::Transaction<'_>, Error> {
        let tx = self.pool.begin().await?;
        Ok(crate::transaction::Transaction {
            tx: Arc::new(tokio::sync::Mutex::new(Some(tx))),
            pool: self.pool.clone(),
            driver: self.driver,
        })
    }

    /// Checks if a table exists in the database.
    pub async fn table_exists(&self, table_name: &str) -> Result<bool, Error> {
        let table_name_snake = table_name.to_snake_case();
        let query = match self.driver {
            Drivers::Postgres => {
                "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public')".to_string()
            }
            Drivers::MySQL => {
                "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = ? AND table_schema = DATABASE())".to_string()
            }
            Drivers::SQLite => {
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?".to_string()
            }
        };

        let row = sqlx::query(&query).bind(&table_name_snake).fetch_one(&self.pool).await?;

        match self.driver {
            Drivers::SQLite => {
                let count: i64 = row.try_get(0)?;
                Ok(count > 0)
            }
            _ => {
                let exists: bool = row.try_get(0)?;
                Ok(exists)
            }
        }
    }

    /// Creates a table based on the provided Model metadata.
    pub async fn create_table<T: Model>(&self) -> Result<(), Error> {
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        let mut query = format!("CREATE TABLE IF NOT EXISTS \"{}\" (", table_name);
        let mut column_defs = Vec::new();
        let mut indexes = Vec::new();

        // Identify primary key columns
        let pk_columns: Vec<String> = columns.iter()
            .filter(|c| c.is_primary_key)
            .map(|c| format!("\"{}\"", c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case()))
            .collect();

        for col in columns {
            let col_name_clean = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
            let mut def = format!("\"{}\" {}", col_name_clean, col.sql_type);

            // If it's a single primary key, we can keep it inline for simplicity
            // If it's composite, we MUST define it as a table constraint
            if col.is_primary_key && pk_columns.len() == 1 {
                def.push_str(" PRIMARY KEY");
            } else if !col.is_nullable || col.is_primary_key {
                def.push_str(" NOT NULL");
            }

            if col.unique && !col.is_primary_key {
                def.push_str(" UNIQUE");
            }

            if col.index && !col.is_primary_key && !col.unique {
                indexes.push(format!(
                    "CREATE INDEX IF NOT EXISTS \"idx_{}_{}\" ON \"{}\" (\"{}\")",
                    table_name, col_name_clean, table_name, col_name_clean
                ));
            }

            column_defs.push(def);
        }

        // Add composite primary key if multiple columns are specified
        if pk_columns.len() > 1 {
            column_defs.push(format!("PRIMARY KEY ({})", pk_columns.join(", ")));
        }

        query.push_str(&column_defs.join(", "));
        query.push(')');

        sqlx::query(&query).execute(&self.pool).await?;

        for idx_query in indexes {
            sqlx::query(&idx_query).execute(&self.pool).await?;
        }

        Ok(())
    }

    /// Synchronizes a table schema by adding missing columns or indexes.
    pub async fn sync_table<T: Model>(&self) -> Result<(), Error> {
        if !self.table_exists(T::table_name()).await? {
            return self.create_table::<T>().await;
        }

        let table_name = T::table_name().to_snake_case();
        let model_columns = T::columns();
        let existing_columns = self.get_table_columns(&table_name).await?;

        for col in model_columns {
            let col_name_clean = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
            if !existing_columns.contains(&col_name_clean) {
                let mut alter_query = format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}", table_name, col_name_clean, col.sql_type);
                if !col.is_nullable {
                    alter_query.push_str(" DEFAULT ");
                    match col.sql_type {
                        "INTEGER" | "INT" | "BIGINT" => alter_query.push('0'),
                        "BOOLEAN" | "BOOL" => alter_query.push_str("FALSE"),
                        _ => alter_query.push_str("''"),
                    }
                }
                sqlx::query(&alter_query).execute(&self.pool).await?;
            }

            if col.index || col.unique {
                let existing_indexes = self.get_table_indexes(&table_name).await?;
                let idx_name = format!("idx_{}_{}", table_name, col_name_clean);
                let uniq_name = format!("unique_{}_{}", table_name, col_name_clean);

                if col.unique && !existing_indexes.contains(&uniq_name) {
                    let mut query = format!("CREATE UNIQUE INDEX \"{}\" ON \"{}\" (\"{}\")", uniq_name, table_name, col_name_clean);
                    if matches!(self.driver, Drivers::SQLite) {
                        query = format!("CREATE UNIQUE INDEX IF NOT EXISTS \"{}\" ON \"{}\" (\"{}\")", uniq_name, table_name, col_name_clean);
                    }
                    sqlx::query(&query).execute(&self.pool).await?;
                } else if col.index && !existing_indexes.contains(&idx_name) && !col.unique {
                    let mut query = format!("CREATE INDEX \"{}\" ON \"{}\" (\"{}\")", idx_name, table_name, col_name_clean);
                    if matches!(self.driver, Drivers::SQLite) {
                        query = format!("CREATE INDEX IF NOT EXISTS \"{}\" ON \"{}\" (\"{}\")", idx_name, table_name, col_name_clean);
                    }
                    sqlx::query(&query).execute(&self.pool).await?;
                }
            }
        }

        Ok(())
    }

    /// Returns the current columns of a table.
    pub async fn get_table_columns(&self, table_name: &str) -> Result<Vec<String>, Error> {
        let table_name_snake = table_name.to_snake_case();
        let query = match self.driver {
            Drivers::Postgres => "SELECT column_name::TEXT FROM information_schema.columns WHERE table_name = $1 AND table_schema = 'public'".to_string(),
            Drivers::MySQL => "SELECT column_name FROM information_schema.columns WHERE table_name = ? AND table_schema = DATABASE()".to_string(),
            Drivers::SQLite => format!("PRAGMA table_info(\"{}\")", table_name_snake),
        };

        let rows = if let Drivers::SQLite = self.driver {
            sqlx::query(&query).fetch_all(&self.pool).await?
        } else {
            sqlx::query(&query).bind(&table_name_snake).fetch_all(&self.pool).await?
        };

        let mut columns = Vec::new();
        for row in rows {
            let col_name: String = if let Drivers::SQLite = self.driver {
                row.try_get("name")?
            } else {
                row.try_get(0)?
            };
            columns.push(col_name);
        }
        Ok(columns)
    }

    /// Returns the current indexes of a table.
    pub async fn get_table_indexes(&self, table_name: &str) -> Result<Vec<String>, Error> {
        let table_name_snake = table_name.to_snake_case();
        let query = match self.driver {
            Drivers::Postgres => "SELECT indexname::TEXT FROM pg_indexes WHERE tablename = $1 AND schemaname = 'public'".to_string(),
            Drivers::MySQL => "SELECT INDEX_NAME FROM information_schema.STATISTICS WHERE TABLE_NAME = ? AND TABLE_SCHEMA = DATABASE()".to_string(),
            Drivers::SQLite => format!("PRAGMA index_list(\"{}\")", table_name_snake),
        };

        let rows = if let Drivers::SQLite = self.driver {
            sqlx::query(&query).fetch_all(&self.pool).await?
        } else {
            sqlx::query(&query).bind(&table_name_snake).fetch_all(&self.pool).await?
        };

        let mut indexes = Vec::new();
        for row in rows {
            let idx_name: String = if let Drivers::SQLite = self.driver {
                row.try_get("name")?
            } else {
                row.try_get(0)?
            };
            indexes.push(idx_name);
        }
        Ok(indexes)
    }

    /// Assigns foreign keys to a table.
    pub async fn assign_foreign_keys<T: Model>(&self) -> Result<(), Error> {
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        for col in columns {
            if let (Some(f_table), Some(f_key)) = (col.foreign_table, col.foreign_key) {
                if matches!(self.driver, Drivers::SQLite) { continue; }
                let constraint_name = format!("fk_{}_{}_{}", table_name, f_table.to_snake_case(), col.name.to_snake_case());
                let query = format!(
                    "ALTER TABLE \"{}\" ADD CONSTRAINT \"{}\" FOREIGN KEY (\"{}\") REFERENCES \"{}\"(\"{}\")",
                    table_name, constraint_name, col.name.to_snake_case(), f_table.to_snake_case(), f_key.to_snake_case()
                );
                let _ = sqlx::query(&query).execute(&self.pool).await;
            }
        }
        Ok(())
    }
}

// ============================================================================
// DatabaseBuilder Struct
// ============================================================================

pub struct DatabaseBuilder {
    max_connections: u32,
}

impl DatabaseBuilder {
    /// Creates a new DatabaseBuilder with default settings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let builder = DatabaseBuilder::new();
    /// ```
    pub fn new() -> Self { Self { max_connections: 5 } }

    /// Sets the maximum number of connections for the database pool.
    ///
    /// # Arguments
    ///
    /// * `max` - The maximum number of connections.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let db = Database::builder()
    ///     .max_connections(10)
    ///     .connect("sqlite::memory:")
    ///     .await?;
    /// ```
    pub fn max_connections(mut self, max: u32) -> Self { self.max_connections = max; self }

    /// Connects to the database using the configured settings.
    ///
    /// # Arguments
    ///
    /// * `url` - The database connection string.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let db = Database::builder()
    ///     .connect("sqlite::memory:")
    ///     .await?;
    /// ```
    pub async fn connect(self, url: &str) -> Result<Database, Error> {
        // Ensure sqlx drivers are registered for Any driver support
        let _ = sqlx::any::install_default_drivers();

        let pool = sqlx::any::AnyPoolOptions::new().max_connections(self.max_connections).connect(url).await?;
        let driver = if url.starts_with("postgres") { Drivers::Postgres }
                    else if url.starts_with("mysql") { Drivers::MySQL }
                    else { Drivers::SQLite };
        Ok(Database { pool, driver })
    }
}

// ============================================================================
// Connection Trait
// ============================================================================

pub trait Connection: Send + Sync {
    fn driver(&self) -> Drivers;
    fn execute<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyQueryResult, sqlx::Error>>;
    fn fetch_all<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Vec<sqlx::any::AnyRow>, sqlx::Error>>;
    fn fetch_one<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyRow, sqlx::Error>>;
    fn fetch_optional<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Option<sqlx::any::AnyRow>, sqlx::Error>>;
    fn clone_db(&self) -> Database;
}

impl Connection for Database {
    fn driver(&self) -> Drivers { self.driver }
    fn execute<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyQueryResult, sqlx::Error>> {
        Box::pin(async move { sqlx::query_with(sql, args).execute(&self.pool).await })
    }
    fn fetch_all<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Vec<sqlx::any::AnyRow>, sqlx::Error>> {
        Box::pin(async move { sqlx::query_with(sql, args).fetch_all(&self.pool).await })
    }
    fn fetch_one<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<sqlx::any::AnyRow, sqlx::Error>> {
        Box::pin(async move { sqlx::query_with(sql, args).fetch_one(&self.pool).await })
    }
    fn fetch_optional<'a, 'q: 'a>(&'a self, sql: &'q str, args: AnyArguments<'q>) -> BoxFuture<'a, Result<Option<sqlx::any::AnyRow>, sqlx::Error>> {
        Box::pin(async move { sqlx::query_with(sql, args).fetch_optional(&self.pool).await })
    }
    fn clone_db(&self) -> Database { self.clone() }
}

// ============================================================================
// Raw SQL Query Builder
// ============================================================================

pub struct RawQuery<'a, C> {
    conn: C,
    sql: &'a str,
    args: AnyArguments<'a>,
}

impl<'a, C> RawQuery<'a, C> where C: Connection {
    pub(crate) fn new(conn: C, sql: &'a str) -> Self {
        Self { conn, sql, args: AnyArguments::default() }
    }

    /// Binds a value to the SQL query.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the value to bind.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = db.raw("SELECT * FROM users WHERE id = ?")
    ///     .bind(1)
    ///     .fetch_one()
    ///     .await?;
    /// ```
    pub fn bind<T>(mut self, value: T) -> Self where T: 'a + sqlx::Encode<'a, sqlx::Any> + sqlx::Type<sqlx::Any> + Send + Sync {
        let _ = self.args.add(value);
        self
    }

    /// Executes the query and returns all matching rows.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type to map the rows to.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = db.raw("SELECT * FROM users")
    ///     .fetch_all()
    ///     .await?;
    /// ```
    pub async fn fetch_all<T>(self) -> Result<Vec<T>, Error> where T: for<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> + Send + Unpin {
        let rows = self.conn.fetch_all(self.sql, self.args).await?;
        Ok(rows.iter().map(|r| T::from_row(r)).collect::<Result<Vec<_>, _>>()?)
    }

    /// Executes the query and returns exactly one row.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type to map the row to.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = db.raw("SELECT * FROM users WHERE id = 1")
    ///     .fetch_one()
    ///     .await?;
    /// ```
    pub async fn fetch_one<T>(self) -> Result<T, Error> where T: for<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> + Send + Unpin {
        let row = self.conn.fetch_one(self.sql, self.args).await?;
        Ok(T::from_row(&row)?)
    }

    /// Executes the query and returns an optional row.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type to map the row to.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = db.raw("SELECT * FROM users WHERE id = 1")
    ///     .fetch_optional()
    ///     .await?;
    /// ```
    pub async fn fetch_optional<T>(self) -> Result<Option<T>, Error> where T: for<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> + Send + Unpin {
        let row = self.conn.fetch_optional(self.sql, self.args).await?;
        Ok(row.map(|r| T::from_row(&r)).transpose()?)
    }

    /// Executes the query and returns the number of affected rows.
    ///
    /// Useful for UPDATE, DELETE or INSERT queries.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let affected = db.raw("DELETE FROM users WHERE id = 1")
    ///     .execute()
    ///     .await?;
    /// ```
    pub async fn execute(self) -> Result<u64, Error> {
        let result = self.conn.execute(self.sql, self.args).await?;
        Ok(result.rows_affected())
    }
}
