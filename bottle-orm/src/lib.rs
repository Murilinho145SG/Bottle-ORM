pub use bottle_orm_macro::Model;
use futures::future::BoxFuture;
use heck::ToSnakeCase;
use sqlx::{any::AnyPoolOptions, AnyPool, Error};

#[derive(Clone)]
pub struct Database {
    pool: AnyPool,
}

pub struct ColumnInfo {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub is_primary_key: bool,
    pub is_nullable: bool,
    pub create_time: bool,
    pub update_time: bool,
    pub unique: bool,
    pub index: bool
}

pub trait Model {
    fn table_name() -> &'static str;
    fn columns() -> Vec<ColumnInfo>;
}

type MigrationTask = Box<dyn Fn(Database) -> BoxFuture<'static, Result<(), sqlx::Error>> + Send + Sync>;

pub struct Migrator<'a> {
    db: &'a Database,
    tasks: Vec<MigrationTask>,
}

impl<'a> Migrator<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db, tasks: Vec::new() }
    }

    pub fn register<T>(mut self) -> Self
    where
        T: Model + 'static + Send + Sync,
    {
        let task = Box::new(|db: Database| -> BoxFuture<'static, Result<(), sqlx::Error>> {
            Box::pin(async move {
                db.auto_migrate::<T>().await?;
                Ok(())
            })
        });
        self.tasks.push(task);
        self
    }

    pub async fn run(self) -> Result<Database, sqlx::Error> {
        for task in self.tasks {
            (task)(self.db.clone()).await?;
        }
        Ok(self.db.clone())
    }
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self, Error> {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(5).connect(url).await?;
        Ok(Self { pool })
    }

    pub fn migrator(&self) -> Migrator {
        Migrator::new(self)
    }

    pub async fn auto_migrate<T: Model>(&self) -> Result<&Self, Error> {
        let table_name = T::table_name();
        let columns = T::columns();
        let mut column_defs = Vec::new();
        for col in columns {
            let name = col.name.strip_prefix("r#").unwrap_or(col.name);
            let mut def = format!("\"{}\" {}", name.to_snake_case(), col.sql_type);

            if col.is_primary_key {
                def.push_str(" PRIMARY KEY");
            }

            if !col.is_nullable {
                def.push_str(" NOT NULL");
            }

            if col.create_time {
                def.push_str(" DEFAULT NOW()");
            }
            
            if col.unique {
            	def.push_str(" UNIQUE");
            }

            column_defs.push(def);
        }

        let query_string =
            format!("CREATE TABLE IF NOT EXISTS \"{}\" ({})", table_name.to_snake_case(), column_defs.join(", "));

        sqlx::query(&query_string).execute(&self.pool).await?;
        Ok(self)
    }
}
