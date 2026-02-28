//! # Migration Module
//!
//! This module provides schema migration management functionality for Bottle ORM.
//! It handles the registration and execution of database schema changes, including
//! table creation and foreign key constraint assignment.
//!
//! ## Overview
//!
//! The migration system follows a two-phase approach:
//!
//! 1. **Table Creation Phase**: Creates all registered tables with their columns,
//!    indexes, and constraints (except foreign keys)
//! 2. **Foreign Key Phase**: Assigns foreign key constraints after all tables exist
//!
//! This ensures that foreign keys can reference tables that haven't been created yet.
//!
//! ## Features
//!
//! - **Automatic Ordering**: Handles dependencies between tables automatically
//! - **Idempotent Operations**: Safe to run multiple times (uses IF NOT EXISTS)
//! - **Type Safety**: Leverages Rust's type system for compile-time validation
//! - **Async Execution**: Non-blocking migration execution
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bottle_orm::{Database, Model};
//! use uuid::Uuid;
//!
//! #[derive(Model)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!     username: String,
//! }
//!
//! #[derive(Model)]
//! struct Post {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!     #[orm(foreign_key = "User::id")]
//!     user_id: Uuid,
//!     title: String,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = Database::connect("postgres://localhost/mydb").await?;
//!
//!     // Register and run migrations
//!     db.migrator()
//!         .register::<User>()
//!         .register::<Post>()
//!         .run()
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use futures::future::BoxFuture;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::{database::Database, model::Model};

// ============================================================================
// Type Aliases
// ============================================================================

/// Type alias for migration tasks (e.g., Create Table, Add Foreign Key).
///
/// Migration tasks are async closures that take a `Database` instance and return
/// a boxed future that resolves to a Result. This allows for flexible, composable
/// migration operations.
///
/// # Type Definition
///
/// ```rust,ignore
/// type MigrationTask = Box<
///     dyn Fn(Database) -> BoxFuture<'static, Result<(), sqlx::Error>> + Send + Sync
/// >;
/// ```
///
/// # Parameters
///
/// * `Database` - Cloned database instance for the migration operation
///
/// # Returns
///
/// * `BoxFuture<'static, Result<(), sqlx::Error>>` - Async result of the migration
///
/// # Traits
///
/// * `Send` - Can be safely sent between threads
/// * `Sync` - Can be safely shared between threads
///
/// # Example
///
/// ```rust,ignore
/// let task: MigrationTask = Box::new(|db: Database| {
///     Box::pin(async move {
///         db.create_table::<User>().await?;
///         Ok(())
///     })
/// });
/// ```
pub type MigrationTask = Box<dyn Fn(Database) -> BoxFuture<'static, Result<(), sqlx::Error>> + Send + Sync>;

// ============================================================================
// Migrator Struct
// ============================================================================

/// Schema migration manager.
///
/// The `Migrator` is responsible for managing and executing database schema migrations.
/// It maintains two separate task queues: one for table creation and one for foreign
/// key assignment. This separation ensures that all tables exist before any foreign
/// keys are created.
///
/// # Fields
///
/// * `db` - Reference to the database connection
/// * `tasks` - Queue of table creation tasks
/// * `fk_task` - Queue of foreign key assignment tasks
///
/// # Lifecycle
///
/// 1. Create migrator via `Database::migrator()`
/// 2. Register models via `register::<T>()`
/// 3. Execute all migrations via `run()`
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::{Database, Model};
///
/// #[derive(Model)]
/// struct User {
///     #[orm(primary_key)]
///     id: i32,
///     username: String,
/// }
///
/// #[derive(Model)]
/// struct Post {
///     #[orm(primary_key)]
///     id: i32,
///     #[orm(foreign_key = "User::id")]
///     user_id: i32,
///     title: String,
/// }
///
/// let db = Database::connect("sqlite::memory:").await?;
///
/// let result = db.migrator()
///     .register::<User>()
///     .register::<Post>()
///     .run()
///     .await?;
/// ```
pub struct Migrator<'a> {
    /// Reference to the database connection.
    ///
    /// This is used to execute migration tasks and is cloned for each task
    /// to allow async execution without lifetime issues.
    pub(crate) db: &'a Database,

    /// Queue of table creation tasks.
    ///
    /// These tasks are executed first, in the order they were registered.
    /// Each task creates a table with its columns, indexes, and constraints
    /// (except foreign keys).
    pub(crate) tasks: Vec<MigrationTask>,

    /// Queue of foreign key assignment tasks.
    ///
    /// These tasks are executed after all table creation tasks complete.
    /// This ensures that referenced tables exist before foreign keys are created.
    pub(crate) fk_task: Vec<MigrationTask>,
}

// ============================================================================
// Migrator Implementation
// ============================================================================

impl<'a> Migrator<'a> {
    // ========================================================================
    // Constructor
    // ========================================================================

    /// Creates a new Migrator instance associated with a Database.
    ///
    /// This constructor initializes empty task queues for table creation
    /// and foreign key assignment. Typically called via `Database::migrator()`
    /// rather than directly.
    ///
    /// # Arguments
    ///
    /// * `db` - Reference to the database connection
    ///
    /// # Returns
    ///
    /// A new `Migrator` instance with empty task queues
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Usually called via database method
    /// let migrator = db.migrator();
    ///
    /// // Direct construction (rarely needed)
    /// let migrator = Migrator::new(&db);
    /// ```
    pub fn new(db: &'a Database) -> Self {
        Self { db, tasks: Vec::new(), fk_task: Vec::new() }
    }

    // ========================================================================
    // Model Registration
    // ========================================================================

    /// Registers a Model for migration.
    ///
    /// This method queues two tasks for the specified model:
    ///
    /// 1. **Table Creation Task**: Creates the table with columns, indexes,
    ///    and inline constraints (PRIMARY KEY, UNIQUE, NOT NULL, etc.)
    /// 2. **Foreign Key Task**: Assigns foreign key constraints after all
    ///    tables are created
    ///
    /// Multiple models can be registered by chaining calls to this method.
    /// The tasks will be executed in the order they were registered.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The Model type to register. Must implement `Model + Send + Sync + 'static`
    ///
    /// # Returns
    ///
    /// Returns `self` to enable method chaining
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use bottle_orm::{Database, Model};
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
    /// #[derive(Model)]
    /// struct Comment {
    ///     #[orm(primary_key)]
    ///     id: Uuid,
    ///     #[orm(foreign_key = "Post::id")]
    ///     post_id: Uuid,
    ///     #[orm(foreign_key = "User::id")]
    ///     user_id: Uuid,
    ///     content: String,
    /// }
    ///
    /// // Register multiple models
    /// db.migrator()
    ///     .register::<User>()      // Creates 'user' table first
    ///     .register::<Post>()      // Creates 'post' table
    ///     .register::<Comment>()   // Creates 'comment' table
    ///     .run()                   // Executes all migrations
    ///     .await?;
    /// ```
    ///
    /// # Task Execution Order
    ///
    /// 1. User table creation
    /// 2. Post table creation
    /// 3. Comment table creation
    /// 4. Post foreign keys (user_id → User.id)
    /// 5. Comment foreign keys (post_id → Post.id, user_id → User.id)
    ///
    /// # See Also
    ///
    /// * [`run()`](#method.run) - For executing registered migrations
    /// * [`Database::create_table()`] - For manual table creation
    /// * [`Database::assign_foreign_keys()`] - For manual FK assignment
    pub fn register<T>(mut self) -> Self
    where
        T: Model + 'static + Send + Sync,
    {
        // Create table creation task with diffing
        let task = Box::new(|db: Database| -> BoxFuture<'static, Result<(), sqlx::Error>> {
            Box::pin(async move {
                // Synchronize table (create if not exists or add missing columns)
                db.sync_table::<T>().await?;
                Ok(())
            })
        });

        // Create foreign key assignment task
        let fk_task = Box::new(|db: Database| -> BoxFuture<'static, Result<(), sqlx::Error>> {
            Box::pin(async move {
                // Assign foreign key constraints
                db.assign_foreign_keys::<T>().await?;
                Ok(())
            })
        });

        // Add tasks to their respective queues
        self.tasks.push(task);
        self.fk_task.push(fk_task);

        // Return self for method chaining
        self
    }

    // ========================================================================
    // Migration Execution
    // ========================================================================

    /// Executes all registered migration tasks.
    ///
    /// This method runs all queued migrations in two phases:
    ///
    /// **Phase 1: Table Creation**
    /// - Executes all table creation tasks in registration order
    /// - Creates tables with columns, indexes, and inline constraints
    /// - Uses `CREATE TABLE IF NOT EXISTS` for idempotency
    ///
    /// **Phase 2: Foreign Key Assignment**
    /// - Executes all foreign key tasks in registration order
    /// - Creates foreign key constraints between tables
    /// - Checks for existing constraints to avoid duplicates
    ///
    /// If any task fails, the entire migration is aborted and an error is returned.
    ///
    /// # Returns
    ///
    /// * `Ok(Database)` - Cloned database instance on success
    /// * `Err(sqlx::Error)` - Database error during migration
    ///
    /// # Error Handling
    ///
    /// Errors can occur for various reasons:
    ///
    /// - **Connection Errors**: Database connection lost during migration
    /// - **Syntax Errors**: Invalid SQL generated (shouldn't happen with correct Model definitions)
    /// - **Permission Errors**: Insufficient database privileges
    /// - **Constraint Violations**: Existing data violates new constraints
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use bottle_orm::{Database, Model};
    ///
    /// #[derive(Model)]
    /// struct User {
    ///     #[orm(primary_key)]
    ///     id: i32,
    ///     username: String,
    /// }
    ///
    /// let db = Database::connect("sqlite::memory:").await?;
    ///
    /// // Run migrations
    /// match db.migrator().register::<User>().run().await {
    ///     Ok(db) => println!("Migrations completed successfully"),
    ///     Err(e) => eprintln!("Migration failed: {}", e),
    /// }
    /// ```
    ///
    /// # Idempotency
    ///
    /// Migrations are designed to be idempotent and can be run multiple times safely:
    ///
    /// ```rust,ignore
    /// // First run: creates tables
    /// db.migrator().register::<User>().run().await?;
    ///
    /// // Second run: no-op (tables already exist)
    /// db.migrator().register::<User>().run().await?;
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - Migrations are executed sequentially, not in parallel
    /// - Large schemas may take time to migrate
    /// - Consider running migrations during deployment/startup
    /// - Use database transactions where supported
    ///
    /// # See Also
    ///
    /// * [`register()`](#method.register) - For registering models
    /// * [`Database::create_table()`] - For manual table creation
    /// * [`Database::assign_foreign_keys()`] - For manual FK assignment
    pub async fn run(self) -> Result<Database, sqlx::Error> {
        // ====================================================================
        // Phase 1: Execute Table Creation Tasks
        // ====================================================================
        // Create all tables in the order they were registered.
        // This ensures that models are created before their dependents.
        for task in self.tasks {
            // Clone the database for the async task
            // This is safe because Database contains a connection pool
            (task)(self.db.clone()).await?;
        }

        // ====================================================================
        // Phase 2: Execute Foreign Key Assignment Tasks
        // ====================================================================
        // Assign foreign keys after all tables exist.
        // This prevents errors where a foreign key references a table
        // that hasn't been created yet.
        for task in self.fk_task {
            // Clone the database for the async task
            (task)(self.db.clone()).await?;
        }

        // Return cloned database instance for continued use
        Ok(self.db.clone())
    }
}
