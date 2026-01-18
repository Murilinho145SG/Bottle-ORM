use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod types;
mod derive_model;

/// Derives the `Model` trait for a struct.
///
/// This macro inspects the struct fields and generates the necessary code to map it to a database table.
/// It supports customization via the `#[orm(...)]` attribute.
///
/// # Supported Attributes
///
/// * `primary_key`: Marks the field as a primary key.
/// * `unique`: Adds a UNIQUE constraint.
/// * `index`: Creates a database index.
/// * `create_time`: Sets default value to CURRENT_TIMESTAMP.
/// * `size = N`: Sets column size (VARCHAR(N)).
/// * `foreign_key = "Table::Column"`: Defines a Foreign Key.
#[proc_macro_derive(Model, attributes(orm))]
pub fn model_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let expanded = derive_model::expand(ast);
    TokenStream::from(expanded)
}