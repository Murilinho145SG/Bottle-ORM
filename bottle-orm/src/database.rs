//! # Database Module
//!
//! This module provides the core database connection and management functionality for Bottle ORM.
//! It handles connection pooling, driver detection, table creation, and foreign key management
//! across multiple database backends.
//!
//! ## Supported Database Drivers
//!
//! - **PostgreSQL**: Full support with advanced features like UUID, JSONB, arrays
//! - **MySQL**: Complete support for standard MySQL/MariaDB features
//! - **SQLite**: In-memory and file-based SQLite databases
//!
//! ## Features
//!
//! - **Connection Pooling**: Automatic connection pool management via sqlx
//! - **Driver Detection**: Automatic database driver detection from connection URL
//! - **Schema Management**: Table creation with indexes, constraints, and foreign keys
//! - **Type Safety**: Type-safe operations across different database backends
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bottle_orm::Database;
//!
//! // Connect to PostgreSQL
//! let db = Database::connect("postgres://user:pass@localhost/mydb").await?;
//!
//! // Connect to SQLite
//! let db = Database::connect("sqlite::memory:").await?;
//!
//! // Connect to MySQL
//! let db = Database::connect("mysql://user:pass@localhost/mydb").await?;
//!
//! // Create table for a model
//! db.create_table::<User>().await?;
//!
//! // Assign foreign keys
//! db.assign_foreign_keys::<Post>().await?;
//!
//! // Start building queries
//! let users = db.model::<User>().scan().await?;
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use heck::ToSnakeCase;
use sqlx::{any::AnyPoolOptions, AnyPool, Error, Row};

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{migration::Migrator, model::Model, query_builder::QueryBuilder, Transaction};

// ============================================================================
// Database Driver Enumeration
// ============================================================================

/// Supported database driver types.
///
/// This enum represents the different database backends that Bottle ORM can work with.
/// The driver type is automatically detected from the connection URL and used to
/// generate appropriate SQL syntax for each database system.
///
/// # Variants
///
/// * `Postgres` - PostgreSQL database (9.5+)
/// * `SQLite` - SQLite database (3.x)
/// * `MySQL` - MySQL or MariaDB database (5.7+/10.2+)
///
/// # SQL Dialect Differences
///
/// Different drivers use different SQL syntax:
///
/// - **Placeholders**:
///   - PostgreSQL: `$1, $2, $3` (numbered)
///   - SQLite/MySQL: `?, ?, ?` (positional)
///
/// - **Type Casting**:
///   - PostgreSQL: `$1::UUID`, `$2::TIMESTAMPTZ`
///   - SQLite/MySQL: Automatic type inference
///
/// - **Schema Queries**:
///   - PostgreSQL: `information_schema` tables
///   - SQLite: `sqlite_master` system table
///   - MySQL: `information_schema` tables
///
/// # Example
///
/// ```rust,ignore
/// match db.driver {
///     Drivers::Postgres => println!("Using PostgreSQL"),
///     Drivers::SQLite => println!("Using SQLite"),
///     Drivers::MySQL => println!("Using MySQL"),
/// }
/// ```
#[derive(Clone, Debug, Copy)]
pub enum Drivers {
    /// PostgreSQL driver.
    ///
    /// Used for PostgreSQL databases. Supports advanced features like:
    /// - UUID native type
    /// - JSONB for JSON data
    /// - Array types
    /// - Full-text search
    /// - Advanced indexing (GiST, GIN, etc.)
    Postgres,

    /// SQLite driver.
    ///
    /// Used for SQLite databases (both in-memory and file-based). Characteristics:
    /// - Lightweight and embedded
    /// - Single-file database
    /// - Limited concurrent write support
    /// - Good for development and small applications
    SQLite,

    /// MySQL driver.
    ///
    /// Used for MySQL and MariaDB databases. Features:
    /// - Wide compatibility
    /// - Good performance for read-heavy workloads
    /// - Mature ecosystem
    /// - ACID compliance (with InnoDB)
    MySQL,
}

// ============================================================================
// Database Connection and Management
// ============================================================================

/// The main entry point for database connection and management.
///
/// `Database` handles connection pooling, driver detection, and provides methods
/// for schema operations and query building. It uses sqlx's `AnyPool` to support
/// multiple database backends with a unified interface.
///
/// # Fields
///
/// * `pool` - The sqlx connection pool for executing queries
/// * `driver` - The detected database driver type
///
/// # Thread Safety
///
/// `Database` implements `Clone` and can be safely shared across threads.
/// The underlying connection pool is thread-safe and handles connection
/// distribution automatically.
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::Database;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Connect to database
///     let db = Database::connect("postgres://localhost/mydb").await?;
///
///     // Create migrator
///     let migrator = db.migrator();
///
///     // Build queries
///     let query = db.model::<User>();
///
///     // Database can be cloned and shared
///     let db_clone = db.clone();
///     tokio::spawn(async move {
///         let users = db_clone.model::<User>().scan().await;
///     });
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Database {
    /// The sqlx connection pool for executing database queries.
    ///
    /// This pool manages a set of database connections that can be reused
    /// across multiple queries, improving performance by avoiding the overhead
    /// of creating new connections for each operation.
    pub(crate) pool: AnyPool,

    /// The detected database driver type.
    ///
    /// Used to generate driver-specific SQL syntax (e.g., placeholders,
    /// type casting, schema queries).
    pub(crate) driver: Drivers,
}

// ============================================================================
// Database Implementation
// ============================================================================

impl Database {
    // ========================================================================
    // Connection Management
    // ========================================================================

    /// Connects to the database using a connection string (Database URL).
    ///
    /// This method establishes a connection pool to the specified database and
    /// automatically detects the driver type based on the URL scheme. The connection
    /// pool is configured with a default maximum of 5 connections.
    ///
    /// # Arguments
    ///
    /// * `url` - The database connection string with the format:
    ///   `<scheme>://<user>:<password>@<host>:<port>/<database>`
    ///
    /// # Supported URL Schemes
    ///
    /// - **PostgreSQL**: `postgres://` or `postgresql://`
    /// - **MySQL**: `mysql://`
    /// - **SQLite**: `sqlite://` or `sqlite::memory:` (for in-memory databases)
    ///
    /// # Connection Pool Configuration
    ///
    /// - Maximum connections: 5
    /// - Connection timeout: Default (30 seconds)
    /// - Idle timeout: Default (10 minutes)
    ///
    /// # Returns
    ///
    /// * `Ok(Database)` - Successfully connected database instance
    /// * `Err(sqlx::Error)` - Connection error (invalid URL, authentication failure, etc.)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // PostgreSQL connection
    /// let db = Database::connect("postgres://user:password@localhost:5432/mydb").await?;
    ///
    /// // PostgreSQL with SSL
    /// let db = Database::connect("postgres://user:password@localhost/mydb?sslmode=require").await?;
    ///
    /// // SQLite in-memory database (great for testing)
    /// let db = Database::connect("sqlite::memory:").await?;
    ///
    /// // SQLite file-based database
    /// let db = Database::connect("sqlite://./database.db").await?;
    ///
    /// // MySQL connection
    /// let db = Database::connect("mysql://user:password@localhost:3306/mydb").await?;
    /// ```
    ///
    /// # Error Handling
    ///
    /// ```rust,ignore
    /// match Database::connect("postgres://localhost/mydb").await {
    ///     Ok(db) => println!("Connected successfully"),
    ///     Err(e) => eprintln!("Connection failed: {}", e),
    /// }
    /// ```
    pub async fn connect(url: &str) -> Result<Self, Error> {
        // Install default drivers for sqlx::Any
        sqlx::any::install_default_drivers();

        // Create connection pool with maximum 5 connections
        let pool = AnyPoolOptions::new().max_connections(5).connect(url).await?;

        // Detect driver type from URL scheme
        let (driver_str, _) = url.split_once(':').unwrap_or(("sqlite", ""));
        let driver = match driver_str {
            "postgresql" | "postgres" => Drivers::Postgres,
            "mysql" => Drivers::MySQL,
            _ => Drivers::SQLite,
        };

        Ok(Self { pool, driver })
    }
    
    // ========================================================================
    // Schema Management
    // ========================================================================

    /// Creates a `Migrator` instance to manage schema migrations.
    ///
    /// The migrator allows you to register multiple models and execute
    /// all necessary schema changes (table creation, foreign keys) in the
    /// correct order.
    ///
    /// # Returns
    ///
    /// A new `Migrator` instance associated with this database connection
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let db = Database::connect("sqlite::memory:").await?;
    ///
    /// db.migrator()
    ///     .register::<User>()
    ///     .register::<Post>()
    ///     .register::<Comment>()
    ///     .run()
    ///     .await?;
    /// ```
    ///
    /// # See Also
    ///
    /// * [`Migrator`] - For detailed migration documentation
    /// * [`Migrator::register()`] - For registering models
    /// * [`Migrator::run()`] - For executing migrations
    pub fn migrator(&self) -> Migrator<'_> {
        Migrator::new(self)
    }

    // ========================================================================
    // Query Building
    // ========================================================================

    /// Starts building a query for a specific Model.
    ///
    /// This method creates a new `QueryBuilder` instance configured for the
    /// specified model type. The query builder provides a fluent interface
    /// for constructing SELECT and INSERT queries.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type to query. Must implement `Model + Send + Sync + Unpin`
    ///
    /// # Returns
    ///
    /// A new `QueryBuilder` instance ready for query construction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Simple query
    /// let users: Vec<User> = db.model::<User>().scan().await?;
    ///
    /// // Filtered query
    /// let adults: Vec<User> = db.model::<User>()
    ///     .filter("age", ">=", 18)
    ///     .scan()
    ///     .await?;
    ///
    /// // Insert operation
    /// let new_user = User { /* ... */ };
    /// db.model::<User>().insert(&new_user).await?;
    /// ```
    ///
    /// # See Also
    ///
    /// * [`QueryBuilder`] - For detailed query building documentation
    /// * [`QueryBuilder::filter()`] - For adding WHERE clauses
    /// * [`QueryBuilder::scan()`] - For executing SELECT queries
    /// * [`QueryBuilder::insert()`] - For INSERT operations
    pub fn model<T: Model + Send + Sync + Unpin>(&self) -> QueryBuilder<'_, T, Self> {
        // Get active column names from the model
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());

        // Convert column names to snake_case and strip 'r#' prefix if present
        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        // Create and return the query builder
        QueryBuilder::new(self.clone(), T::table_name(), T::columns(), columns)
    }

    // ========================================================================
    // Table Creation
    // ========================================================================

    /// Creates the table for model `T` if it does not exist.
    ///
    /// This method generates and executes SQL to create a table based on the
    /// model's structure. It handles column definitions, primary keys, unique
    /// constraints, default values, and indexes.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type representing the table
    ///
    /// # Returns
    ///
    /// * `Ok(&Self)` - Reference to self for method chaining
    /// * `Err(sqlx::Error)` - Database error during table creation
    ///
    /// # Generated SQL Features
    ///
    /// - **Primary Keys**: Automatically marked with `PRIMARY KEY`
    /// - **NOT NULL**: Non-nullable fields get `NOT NULL` constraint
    /// - **UNIQUE**: Fields marked with `#[orm(unique)]` get `UNIQUE` constraint
    /// - **DEFAULT**: Fields marked with `#[orm(create_time)]` get `DEFAULT CURRENT_TIMESTAMP`
    /// - **Indexes**: Fields marked with `#[orm(index)]` get database indexes
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use bottle_orm::Model;
    /// use uuid::Uuid;
    /// use chrono::{DateTime, Utc};
    ///
    /// #[derive(Model)]
    /// struct User {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     #[orm(size = 50, unique)]
    ///     username: String,
    ///     #[orm(index)]
    ///     email: String,
    ///     age: i32,
    ///     #[orm(create_time)]
    ///     created_at: DateTime<Utc>,
    /// }
    ///
    /// // Creates table with:
    /// // - UUID primary key
    /// // - Unique username constraint
    /// // - Index on email
    /// // - created_at with DEFAULT CURRENT_TIMESTAMP
    /// db.create_table::<User>().await?;
    /// ```
    ///
    /// # Generated SQL Example (PostgreSQL)
    ///
    /// ```sql
    /// CREATE TABLE IF NOT EXISTS "user" (
    ///     "id" UUID PRIMARY KEY,
    ///     "username" VARCHAR(50) NOT NULL UNIQUE,
    ///     "email" TEXT NOT NULL,
    ///     "age" INTEGER NOT NULL,
    ///     "created_at" TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    /// );
    /// CREATE INDEX IF NOT EXISTS "idx_user_email" ON "user" ("email");
    /// ```
    pub async fn create_table<T: Model>(&self) -> Result<&Self, Error> {
        // Get table name in snake_case format
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        let mut column_defs = Vec::new();
        let mut index_statements = Vec::new();

        // Build column definitions
        for col in &columns {
            // Strip 'r#' prefix if present (for Rust keywords used as column names)
            let col_name = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
            let mut def = format!("\"{}\" {}", col_name, col.sql_type);

            // Add PRIMARY KEY constraint
            if col.is_primary_key {
                def.push_str(" PRIMARY KEY");
            }

            // Add NOT NULL constraint (except for primary keys, which are implicitly NOT NULL)
            if !col.is_nullable && !col.is_primary_key {
                def.push_str(" NOT NULL");
            }

            // Add DEFAULT CURRENT_TIMESTAMP for create_time fields
            if col.create_time {
                def.push_str(" DEFAULT CURRENT_TIMESTAMP");
            }

            // Add UNIQUE constraint
            if col.unique {
                def.push_str(" UNIQUE");
            }

            column_defs.push(def);

            // Generate index creation statement if needed
            if col.index {
                let index_type = if col.unique { "UNIQUE INDEX" } else { "INDEX" };
                let index_name = format!("idx_{}_{}", table_name, col_name);

                let index_query = format!(
                    "CREATE {} IF NOT EXISTS \"{}\" ON \"{}\" (\"{}\")",
                    index_type, index_name, table_name, col_name,
                );

                index_statements.push(index_query);
            }
        }

        // Build and execute CREATE TABLE statement
        let create_table_query =
            format!("CREATE TABLE IF NOT EXISTS \"{}\" ({})", table_name.to_snake_case(), column_defs.join(", "));
        log::info!("{}", create_table_query);

        sqlx::query(&create_table_query).execute(&self.pool).await?;

        // Create indexes
        for idx_stmt in index_statements {
            sqlx::query(&idx_stmt).execute(&self.pool).await?;
        }

        Ok(self)
    }

    /// Starts a new database transaction.
    ///
    /// Returns a `Transaction` wrapper that can be used to execute multiple
    /// queries atomically. The transaction must be explicitly committed
    /// using `commit()`, otherwise it will be rolled back when dropped.
    ///
    /// # Returns
    ///
    /// * `Ok(Transaction)` - A new transaction instance
    /// * `Err(sqlx::Error)` - Database error starting transaction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut tx = db.begin().await?;
    /// // ... perform operations ...
    /// tx.commit().await?;
    /// ```
    pub async fn begin<'a>(&self) -> Result<Transaction<'a>, sqlx::Error> {
        let tx = self.pool.begin().await?;
        Ok(Transaction { tx, driver: self.driver })
    }

    // ========================================================================
    // Foreign Key Management
    // ========================================================================

    /// Checks for and assigns Foreign Keys for model `T`.
    ///
    /// This method examines all columns marked with `#[orm(foreign_key = "Table::Column")]`
    /// and creates the appropriate foreign key constraints. It verifies that constraints
    /// don't already exist before attempting to create them, preventing duplication errors.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type to process for foreign keys
    ///
    /// # Returns
    ///
    /// * `Ok(&Self)` - Reference to self for method chaining
    /// * `Err(sqlx::Error)` - Database error during foreign key creation
    ///
    /// # Constraint Naming
    ///
    /// Foreign key constraints are named using the pattern:
    /// `fk_{table_name}_{column_name}`
    ///
    /// # Example
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
    /// }
    ///
    /// #[derive(Model)]
    /// struct Post {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     #[orm(foreign_key = "User::id")]
    ///     user_id: Uuid,
    ///     title: String,
    /// }
    ///
    /// // Create tables first
    /// db.create_table::<User>().await?;
    /// db.create_table::<Post>().await?;
    ///
    /// // Then assign foreign keys
    /// db.assign_foreign_keys::<Post>().await?;
    /// ```
    ///
    /// # Generated SQL Example
    ///
    /// ```sql
    /// ALTER TABLE "post"
    /// ADD CONSTRAINT "fk_post_user_id"
    /// FOREIGN KEY ("user_id")
    /// REFERENCES "user" ("id");
    /// ```
    ///
    /// # Important Notes
    ///
    /// - Foreign key assignment should be done **after** all tables are created
    /// - The referenced table and column must exist before creating the foreign key
    /// - Use the `Migrator` to handle the correct order automatically
    /// - Currently optimized for PostgreSQL (uses `information_schema`)
    ///
    /// # See Also
    ///
    /// * [`Migrator`] - For automatic migration order management
    pub async fn assign_foreign_keys<T: Model>(&self) -> Result<&Self, Error> {
        // Get table name in snake_case format
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        // Process each column that has a foreign key definition
        for col in columns {
            if let (Some(f_table), Some(f_key)) = (col.foreign_table, col.foreign_key) {
                // Clean up column and reference names
                let col_name = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
                let f_table_clean = f_table.to_snake_case();
                let f_key_clean = f_key.to_snake_case();

                // Generate constraint name
                let constraint_name = format!("fk_{}_{}", table_name, col_name);

                // Check if constraint already exists (PostgreSQL-specific)
                // TODO: Add support for MySQL and SQLite constraint checking
                let check_query =
                    "SELECT count(*) FROM information_schema.table_constraints WHERE constraint_name = $1";
                let row = sqlx::query(check_query).bind(&constraint_name).fetch_one(&self.pool).await?;

                let count: i64 = row.try_get(0).unwrap_or(0);

                // Skip if constraint already exists
                if count > 0 {
                    continue;
                }

                // Create foreign key constraint
                let alter_query = format!(
                    "ALTER TABLE \"{}\" ADD CONSTRAINT \"{}\" FOREIGN KEY (\"{}\") REFERENCES \"{}\" (\"{}\")",
                    table_name, constraint_name, col_name, f_table_clean, f_key_clean
                );

                sqlx::query(&alter_query).execute(&self.pool).await?;
            }
        }

        Ok(self)
    }
}

/// A trait representing a database connection or transaction.
///
/// This trait abstracts over `Database` (pool) and `Transaction` types, allowing
/// the `QueryBuilder` to work seamlessly with both. It uses Generic Associated Types (GATs)
/// to handle the lifetimes of the executor references correctly.
pub trait Connection {
    /// The type of the executor returned by this connection.
    ///
    /// This uses GATs to bind the lifetime of the executor (`'c`) to the lifetime
    /// of the borrow of the connection (`&'c mut self`).
    type Exec<'c>: sqlx::Executor<'c, Database = sqlx::Any>
    where
        Self: 'c;

    /// Returns a mutable reference to the SQLx executor.
    ///
    /// # Returns
    ///
    /// An executor capable of running SQL queries (either a Pool or a Transaction).
    fn executor<'c>(&'c mut self) -> Self::Exec<'c>;

    /// Returns the driver type associated with this connection.
    fn driver(&self) -> Drivers;
}

/// Implementation of Connection for the main Database struct.
///
/// Uses the internal connection pool to execute queries.
impl Connection for Database {
    type Exec<'c> = &'c sqlx::Pool<sqlx::Any>;

    fn driver(&self) -> Drivers {
        self.driver
    }

    fn executor<'c>(&'c mut self) -> Self::Exec<'c> {
        &self.pool
    }
}
