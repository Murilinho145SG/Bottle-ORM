use sqlx::{
    any::{AnyPoolOptions, AnyRow},
    AnyPool, Error, Row,
};
use heck::ToSnakeCase;
use crate::{model::Model, query_builder::QueryBuilder, migration::Migrator};
use std::marker::PhantomData;

/// Supported database driver types.
#[derive(Clone, Debug)]
pub enum Drivers {
    /// PostgreSQL driver.
    Postgres,
    /// SQLite driver.
    SQLite,
    /// MySQL driver.
    MySQL,
}

/// The main entry point for database connection and management.
///
/// It handles connection pooling, driver detection, and schema operations.
#[derive(Clone)]
pub struct Database {
    pub(crate) pool: AnyPool,
    pub(crate) driver: Drivers,
}

impl Database {
    /// Connects to the database using a connection string (Database URL).
    ///
    /// It automatically identifies the driver (Postgres, MySQL, SQLite) based on the URL prefix.
    ///
    /// # Arguments
    ///
    /// * `url` - The database connection string (e.g., `postgres://user:pass@localhost/db`).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let db = Database::connect("postgres://user:password@localhost/mydb").await?;
    /// ```
    pub async fn connect(url: &str) -> Result<Self, Error> {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(5).connect(url).await?;
        
        let (driver_str, _) = url.split_once(":").unwrap_or(("sqlite", ""));
        let driver = match driver_str {
            "postgresql" | "postgres" => Drivers::Postgres,
            "mysql" => Drivers::MySQL,
            _ => Drivers::SQLite,
        };
        
        Ok(Self { pool, driver })
    }

    /// Creates a `Migrator` instance to manage schema migrations.
    pub fn migrator(&self) -> Migrator<'_> {
        Migrator::new(self)
    }

    /// Starts building a query for a specific Model.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query = db.model::<User>();
    /// ```
    pub fn model<T: Model>(&self) -> QueryBuilder<'_, T> {
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());       
        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        QueryBuilder::new(self, T::table_name(), T::columns(), columns)
    }

    /// Creates the table for model `T` if it does not exist.
    ///
    /// This method generates SQL for columns, primary keys, indexes, and constraints.
    pub async fn create_table<T: Model>(&self) -> Result<&Self, Error> {
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        let mut column_defs = Vec::new();
        let mut index_statements = Vec::new();

        for col in &columns {
            let col_name = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case(); 
            let mut def = format!("\"{}\" {}\"", col_name, col.sql_type);

            if col.is_primary_key {
                def.push_str(" PRIMARY KEY");
            }

            if !col.is_nullable && !col.is_primary_key {
                def.push_str(" NOT NULL");
            }

            if col.create_time {
                def.push_str(" DEFAULT CURRENT_TIMESTAMP");
            }

            if col.unique {
                def.push_str(" UNIQUE");
            }

            column_defs.push(def);

            if col.index {
                let index_type = if col.unique { "UNIQUE INDEX" } else { "INDEX" };
                let index_name = format!("idx_{}_{}", table_name, col_name);

                let index_query = format!(
                    "CREATE {} IF NOT EXISTS \"{}\" ON \"{}\" (\"{}\" )",
                    index_type, index_name, table_name, col_name,
                );

                index_statements.push(index_query);
            }
        }

        let create_table_query =
            format!("CREATE TABLE IF NOT EXISTS \"{}\" ({})", table_name.to_snake_case(), column_defs.join(", "));

        sqlx::query(&create_table_query).execute(&self.pool).await?;
        for idx_stmt in index_statements {
            sqlx::query(&idx_stmt).execute(&self.pool).await?;
        }
        Ok(self)
    }

    /// Checks for and assigns Foreign Keys for model `T`.
    ///
    /// This function verifies if the constraint already exists before attempting to create it
    /// to avoid duplication errors.
    pub async fn assign_foreign_keys<T: Model>(&self) -> Result<&Self, Error> {
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        for col in columns {
            if let (Some(f_table), Some(f_key)) = (col.foreign_table, col.foreign_key) {    
                let col_name = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
                let f_table_clean = f_table.to_snake_case();
                let f_key_clean = f_key.to_snake_case();

                let constraint_name = format!("fk_{}_{}", table_name, col_name);

                let check_query =
                    "SELECT count(*) FROM information_schema.table_constraints WHERE constraint_name = $1";
                let row = sqlx::query(check_query).bind(&constraint_name).fetch_one(&self.pool).await?;
                let count: i64 = row.try_get(0).unwrap_or(0);
                if count > 0 {
                    continue;
                }

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