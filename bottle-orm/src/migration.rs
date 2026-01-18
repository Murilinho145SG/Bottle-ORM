use crate::{database::Database, model::Model};
use futures::future::BoxFuture;

/// Type alias for migration tasks (e.g., Create Table, Add Foreign Key).
///
/// These tasks are closures that take a `Database` connection and return a future.
pub type MigrationTask = Box<dyn Fn(Database) -> BoxFuture<'static, Result<(), sqlx::Error>> + Send + Sync>;

/// Schema migration manager.
///
/// Handles the registration of models and executes table creation and relationship setup in order.
pub struct Migrator<'a> {
    pub(crate) db: &'a Database,
    pub(crate) tasks: Vec<MigrationTask>,
    pub(crate) fk_task: Vec<MigrationTask>,
}

impl<'a> Migrator<'a> {
    /// Creates a new Migrator instance associated with a Database.
    pub fn new(db: &'a Database) -> Self {
        Self { db, tasks: Vec::new(), fk_task: Vec::new() }
    }

    /// Registers a Model for migration.
    ///
    /// This queues tasks to:
    /// 1. Create the table for the model.
    /// 2. Assign foreign keys (executed later to ensure all tables exist).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.migrator()
    ///   .register::<User>()
    ///   .register::<Post>()
    ///   .run()
    ///   .await?;
    /// ```
    pub fn register<T>(mut self) -> Self
    where
        T: Model + 'static + Send + Sync,
    {
        let task = Box::new(|db: Database| -> BoxFuture<'static, Result<(), sqlx::Error>> { 
            Box::pin(async move {
                db.create_table::<T>().await?;
                Ok(())
            })
        });

        let fk_task = Box::new(|db: Database| -> BoxFuture<'static, Result<(), sqlx::Error>> {
            Box::pin(async move {
                db.assign_foreign_keys::<T>().await?;
                Ok(())
            })
        });
        self.tasks.push(task);
        self.fk_task.push(fk_task);
        self
    }

    /// Executes all registered migration tasks.
    ///
    /// The process follows two steps:
    /// 1. Creates all tables (executing standard migration tasks).
    /// 2. Creates all foreign keys (executing foreign key tasks).
    pub async fn run(self) -> Result<Database, sqlx::Error> {
        for task in self.tasks {
            (task)(self.db.clone()).await?;
        }

        for task in self.fk_task {
            (task)(self.db.clone()).await?;
        }
        Ok(self.db.clone())
    }
}