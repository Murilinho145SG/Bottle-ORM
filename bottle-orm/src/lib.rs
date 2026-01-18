use std::marker::PhantomData;

pub use bottle_orm_macro::Model;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use futures::future::BoxFuture;
use heck::ToSnakeCase;
use sqlx::{
    any::{AnyArguments, AnyPoolOptions, AnyRow},
    query::Query,
    Any, AnyPool, Error, FromRow, Row,
};

#[derive(Clone)]
enum Drivers {
    Postgres,
    SQLite,
    MySQL,
}

#[derive(Clone)]
pub struct Database {
    pool: AnyPool,
    driver: Drivers,
}

pub struct ColumnInfo {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub is_primary_key: bool,
    pub is_nullable: bool,
    pub create_time: bool,
    pub update_time: bool,
    pub unique: bool,
    pub index: bool,
    pub foreign_table: Option<&'static str>,
    pub foreign_key: Option<&'static str>,
}

pub trait Model {
    fn table_name() -> &'static str;
    fn columns() -> Vec<ColumnInfo>;
    fn active_columns() -> Vec<&'static str>;
    fn to_map(&self) -> std::collections::HashMap<String, String>;
    // fn bind_values<'q>(&'q self, query: Query<'q, Any, AnyArguments<'q>>) -> Query<'q, Any, AnyArguments<'q>>;
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self, Error> {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(5).connect(url).await?;
        let (driver_str, _) = url.split_once(":").unwrap();
        let driver = match driver_str {
            "postgresql" => Drivers::Postgres,
            "mysql" => Drivers::MySQL,
            _ => Drivers::SQLite,
        };
        Ok(Self { pool, driver })
    }

    pub fn migrator(&self) -> Migrator<'_> {
        Migrator::new(self)
    }

    pub fn model<T: Model>(&self) -> QueryBuilder<'_, T> {
        let active_columns = T::active_columns();
        let mut columns: Vec<String> = Vec::with_capacity(active_columns.capacity());
        for col in active_columns {
            columns.push(col.strip_prefix("r#").unwrap_or(col).to_snake_case());
        }

        QueryBuilder {
            db: self,
            table_name: &T::table_name(),
            columns_info: T::columns(),
            columns: columns,
            select_columns: Vec::new(),
            where_clauses: Vec::new(),
            order_clauses: Vec::new(),
            limit: None,
            offset: None,
            _marker: PhantomData,
        }
    }

    pub async fn create_table<T: Model>(&self) -> Result<&Self, Error> {
        let table_name = T::table_name().to_snake_case();
        let columns = T::columns();

        let mut column_defs = Vec::new();
        let mut index_statements = Vec::new();

        for col in &columns {
            let col_name = col.name.strip_prefix("r#").unwrap_or(col.name).to_snake_case();
            let mut def = format!("\"{}\" {}", col_name, col.sql_type);

            if col.is_primary_key {
                def.push_str(" PRIMARY KEY");
            }

            if !col.is_nullable {
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
                    "CREATE {} IF NOT EXISTS \"{}\" ON \"{}\" (\"{}\")",
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

type FilterFn = Box<dyn Fn(&mut String, &mut AnyArguments<'_>, &Drivers, &mut usize) + Send + Sync>;

pub struct QueryBuilder<'a, T> {
    db: &'a Database,
    table_name: &'static str,
    columns_info: Vec<ColumnInfo>,
    columns: Vec<String>,
    select_columns: Vec<String>,
    where_clauses: Vec<FilterFn>,
    order_clauses: Vec<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    _marker: PhantomData<T>,
}

impl<'a, T: Model + Send + Sync + Unpin> QueryBuilder<'a, T> {
    pub fn filter<V>(mut self, col: &'static str, op: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> sqlx::Encode<'q, Any> + sqlx::Type<Any> + Send + Sync + Clone,
    {
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND \"");
            query.push_str(col);
            query.push_str("\" ");
            query.push_str(op);
            query.push(' ');

            match driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${}", arg_counter));
                    *arg_counter += 1;
                }
                _ => query.push('?'),
            }
            
            use sqlx::Arguments;
            args.add(value.clone());
        });
        self.where_clauses.push(clause);
        self
    }

    pub fn select(mut self, columns: &str) -> Self {
        self.select_columns.push(columns.to_string());
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub async fn insert(&self, model: &T) -> Result<&Self, sqlx::Error> {
        let data_map = model.to_map();

        if data_map.is_empty() {
            return Ok(&self);
        }

        let table_name = self.table_name.to_snake_case();
        let columns_info = T::columns();

        let mut target_columns = Vec::new();
        let mut bindings: Vec<(String, &str)> = Vec::new();

        for (col_name, value) in data_map {
            let col_name_clean = col_name.strip_prefix("r#").unwrap_or(&col_name).to_snake_case();
            target_columns.push(format!("\"{}\"", col_name_clean));

            let sql_type = columns_info.iter().find(|c| c.name == col_name).map(|c| c.sql_type).unwrap_or("TEXT");

            bindings.push((value, sql_type));
        }

        let placeholders: Vec<String> = bindings
            .iter()
            .enumerate()
            .map(|(i, (_, sql_type))| match self.db.driver {
                Drivers::Postgres => {
                    let idx = i + 1;
                    match *sql_type {
                        "TIMESTAMPTZ" | "DateTime" => format!("${}::TIMESTAMPTZ", idx),
                        "TIMESTAMP" | "NaiveDateTime" => format!("${}::TIMESTAMP", idx),
                        "DATE" | "NaiveDate" => format!("${}::DATE", idx),
                        "TIME" | "NaiveTime" => format!("${}::TIME", idx),
                        _ => format!("${}", idx),
                    }
                }
                _ => "?".to_string(),
            })
            .collect();

        let query_str = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            table_name,
            target_columns.join(", "),
            placeholders.join(", ")
        );

        println!("{}", query_str);
        let mut query = sqlx::query::<sqlx::Any>(&query_str);
        for (val_str, sql_type) in bindings {
            match sql_type {
                "INTEGER" | "INT" | "SERIAL" | "serial" | "int4" => {
                    let val: i32 = val_str.parse().unwrap_or_default();
                    query = query.bind(val);
                }
                "BIGINT" | "INT8" | "int8" => {
                    let val: i64 = val_str.parse().unwrap_or_default();
                    query = query.bind(val);
                }
                "BOOLEAN" | "BOOL" | "bool" => {
                    let val: bool = val_str.parse().unwrap_or(false);
                    query = query.bind(val);
                }
                "DOUBLE PRECISION" | "FLOAT" | "float8" => {
                    let val: f64 = val_str.parse().unwrap_or_default();
                    query = query.bind(val);
                }
                "TIMESTAMP" | "NaiveDateTime" => {
                    if let Ok(val) = val_str.parse::<NaiveDateTime>() {
                        query = query.bind(val.to_string());
                    } else {
                        query = query.bind(val_str);
                    }
                }
                "TIMESTAMPTZ" | "DateTime" => {
                    if let Ok(val) = val_str.parse::<DateTime<Utc>>() {
                        query = query.bind(val.to_string());
                    } else {
                        query = query.bind(val_str);
                    }
                }
                "DATE" | "NaiveDate" => {
                    if let Ok(val) = val_str.parse::<NaiveDate>() {
                        query = query.bind(val.to_string());
                    } else {
                        query = query.bind(val_str);
                    }
                }
                "TIME" | "NaiveTime" => {
                    if let Ok(val) = val_str.parse::<NaiveTime>() {
                        query = query.bind(val.to_string());
                    } else {
                        query = query.bind(val_str);
                    }
                }
                _ => query = query.bind(val_str),
            }
        }

        query.execute(&self.db.pool).await?;
        Ok(&self)
    }

    fn to_sql(&self) -> String {
        let mut query = String::from("SELECT ");
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");
        
        // Note: to_sql cannot easily show bindings without context, 
        // so we just run the clauses with a dummy args to build the string
        // This is mainly for debug or non-bound usage if any.
        let mut dummy_args = AnyArguments::default();
        let mut dummy_counter = 1;
        
        for clause in &self.where_clauses {
            clause(&mut query, &mut dummy_args, &self.db.driver, &mut dummy_counter);
        }

        query
    }

    pub async fn scan<R>(self) -> Result<Vec<R>, sqlx::Error>
    where
        R: for<'r> FromRow<'r, AnyRow> + Send + Unpin,
    {
        let mut query = String::from("SELECT ");
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");

        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.db.driver, &mut arg_counter);
        }

        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ");
            match self.db.driver {
                 Drivers::Postgres => {
                     query.push_str(&format!("${}", arg_counter));
                     arg_counter += 1;
                 }
                 _ => query.push('?'),
            }
            use sqlx::Arguments;
            args.add(limit as i64);
        }

        if let Some(offset) = self.offset {
            query.push_str(" OFFSET ");
            match self.db.driver {
                 Drivers::Postgres => {
                     query.push_str(&format!("${}", arg_counter));
                     arg_counter += 1;
                 }
                 _ => query.push('?'),
            }
            use sqlx::Arguments;
            args.add(offset as i64);
        }

        sqlx::query_as_with::<_, R, _>(&query, args).fetch_all(&self.db.pool).await
    }

    pub async fn first<R>(self) -> Result<R, sqlx::Error>
    where
        R: for<'r> FromRow<'r, AnyRow> + Send + Unpin,
    {
        let mut query = String::from("SELECT ");
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");

        let mut args = AnyArguments::default();
        let mut arg_counter = 1;

        for clause in &self.where_clauses {
            clause(&mut query, &mut args, &self.db.driver, &mut arg_counter);
        }

        let pk_column = T::columns()
            .iter()
            .find(|c| c.is_primary_key)
            .map(|c| c.name.strip_prefix("r#").unwrap_or(c.name).to_snake_case());

        if let Some(pk) = pk_column {
            query.push_str(" ORDER BY \"");
            query.push_str(&pk);
            query.push_str("\" ASC");
        }

        query.push_str(" LIMIT 1");

        sqlx::query_as_with::<_, R, _>(&query, args).fetch_one(&self.db.pool).await
    }
}
type MigrationTask = Box<dyn Fn(Database) -> BoxFuture<'static, Result<(), sqlx::Error>> + Send + Sync>;

pub struct Migrator<'a> {
    db: &'a Database,
    tasks: Vec<MigrationTask>,
    fk_task: Vec<MigrationTask>,
}

impl<'a> Migrator<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db, tasks: Vec::new(), fk_task: Vec::new() }
    }

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
