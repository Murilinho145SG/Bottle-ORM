pub use bottle_orm_macro::Model;

// Declaração dos módulos
pub mod database;
pub mod model;
pub mod query_builder;
pub mod migration;

// Re-exportação para facilitar o uso pela API pública
pub use database::Database;
pub use model::{Model, ColumnInfo};
pub use query_builder::QueryBuilder;
pub use migration::Migrator;