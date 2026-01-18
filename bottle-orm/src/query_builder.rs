use crate::{database::{Database, Drivers}, model::{ColumnInfo, Model}};
use sqlx::{
    any::{AnyArguments, AnyRow},
    Any, Arguments, FromRow, Type, Encode,
};
use heck::ToSnakeCase;
use std::marker::PhantomData;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};

/// A type alias for filter closures that support manual SQL construction and argument binding.
///
/// It receives:
/// 1. `&mut String`: The SQL query buffer being built.
/// 2. `&mut AnyArguments`: The argument container for binding values.
/// 3. `&Drivers`: The current database driver (to decide between `$n` or `?`).
/// 4. `&mut usize`: The argument counter (for PostgreSQL `$n` placeholders).
pub type FilterFn = Box<dyn Fn(&mut String, &mut AnyArguments<'_>, &Drivers, &mut usize) + Send + Sync>;

/// A fluent Query Builder for constructing SQL queries.
///
/// Handles SELECT, INSERT, filtering (WHERE), pagination (LIMIT/OFFSET), and ordering.
pub struct QueryBuilder<'a, T> {
    pub(crate) db: &'a Database,
    pub(crate) table_name: &'static str,
    pub(crate) columns_info: Vec<ColumnInfo>,
    pub(crate) columns: Vec<String>,
    pub(crate) select_columns: Vec<String>,
    pub(crate) where_clauses: Vec<FilterFn>,
    pub(crate) order_clauses: Vec<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
    pub(crate) _marker: PhantomData<T>,
}

impl<'a, T: Model + Send + Sync + Unpin> QueryBuilder<'a, T> {
    /// Creates a new QueryBuilder instance.
    ///
    /// Usually called via `db.model::<T>()`.
    pub fn new(
        db: &'a Database, 
        table_name: &'static str, 
        columns_info: Vec<ColumnInfo>, 
        columns: Vec<String>
    ) -> Self {
        Self {
            db,
            table_name,
            columns_info,
            columns,
            select_columns: Vec::new(),
            where_clauses: Vec::new(),
            order_clauses: Vec::new(),
            limit: None,
            offset: None,
            _marker: PhantomData,
        }
    }

    /// Adds a WHERE clause to the query.
    ///
    /// # Arguments
    ///
    /// * `col` - The column name.
    /// * `op` - The operator (e.g., "=", ">", "LIKE").
    /// * `value` - The value to compare against.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// db.model::<User>().filter("age", ">", 18).scan().await?;
    /// ```
    pub fn filter<V>(mut self, col: &'static str, op: &'static str, value: V) -> Self
    where
        V: 'static + for<'q> Encode<'q, Any> + Type<Any> + Send + Sync + Clone, 
    {
        let clause: FilterFn = Box::new(move |query, args, driver, arg_counter| {
            query.push_str(" AND \"");
            query.push_str(col);
            query.push_str("\" ");
            query.push_str(op);
            query.push(' ');

            match driver {
                Drivers::Postgres => {
                    query.push_str(&format!("${{}}", arg_counter));
                    *arg_counter += 1;
                }
                _ => query.push('?'),
            }
            
            args.add(value.clone());
        });
        self.where_clauses.push(clause);
        self
    }

    /// Selects specific columns to return.
    ///
    /// By default, all columns (`*`) are selected.
    pub fn select(mut self, columns: &str) -> Self {
        self.select_columns.push(columns.to_string());
        self
    }

    /// Sets the query offset (pagination).
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Sets the maximum number of records to return.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Inserts a new record into the database based on the model instance.
    ///
    /// Uses manual string parsing to bind values (temporary solution until fuller serialization support).
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
            target_columns.push(format!("\"{{}}\"", col_name_clean));

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
                        "TIMESTAMPTZ" | "DateTime" => format!("${{}}::TIMESTAMPTZ", idx),     
                        "TIMESTAMP" | "NaiveDateTime" => format!("${{}}::TIMESTAMP", idx),    
                        "DATE" | "NaiveDate" => format!("${{}}::DATE", idx),
                        "TIME" | "NaiveTime" => format!("${{}}::TIME", idx),
                        _ => format!("${{}}", idx),
                    }
                }
                _ => "?".to_string(),
            })
            .collect();

        let query_str = format!(
            "INSERT INTO \"{{}}\" ({{}}) VALUES ({{}})",
            table_name,
            target_columns.join(", "),
            placeholders.join(", ")
        );

        // println!("{}", query_str); // Debug if needed
        let mut query = sqlx::query::<sqlx::Any>(&query_str);
        
        // Manual binding based on string parsing
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

    /// Returns the generated SQL string (for debugging purposes, without arguments).
    pub fn to_sql(&self) -> String {
        let mut query = String::from("SELECT ");
        if self.select_columns.is_empty() {
            query.push('*');
        } else {
            query.push_str(&self.select_columns.join(", "));
        }
        query.push_str(" FROM \"");
        query.push_str(&self.table_name.to_snake_case());
        query.push_str("\" WHERE 1=1");
        
        let mut dummy_args = AnyArguments::default();
        let mut dummy_counter = 1;
        
        for clause in &self.where_clauses {
            clause(&mut query, &mut dummy_args, &self.db.driver, &mut dummy_counter);
        }

        query
    }

    /// Executes the query and returns a list of results.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = db.model::<User>().scan().await?;
    /// ```
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
                     query.push_str(&format!("${{}}", arg_counter));
                     arg_counter += 1;
                 }
                 _ => query.push('?'),
            }
            args.add(limit as i64);
        }

        if let Some(offset) = self.offset {
            query.push_str(" OFFSET ");
            match self.db.driver {
                 Drivers::Postgres => {
                     query.push_str(&format!("${{}}", arg_counter));
                     arg_counter += 1;
                 }
                 _ => query.push('?'),
            }
            args.add(offset as i64);
        }

        sqlx::query_as_with::<_, R, _>(&query, args).fetch_all(&self.db.pool).await
    }

    /// Executes the query and returns only the first result.
    ///
    /// Automatically adds `LIMIT 1` and orders by Primary Key if available.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = db.model::<User>().filter("id", "=", 1).first().await?;
    /// ```
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