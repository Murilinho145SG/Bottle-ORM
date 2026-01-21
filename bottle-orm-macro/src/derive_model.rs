//! # Model Derive Macro Implementation
//!
//! This module implements the procedural macro expansion for `#[derive(Model)]`.
//! It analyzes struct fields and their `#[orm(...)]` attributes to automatically
//! generate the `Model` trait implementation.
//!
//! ## Overview
//!
//! The derive macro performs the following tasks:
//!
//! 1. **Field Analysis**: Examines each struct field to determine its type and attributes
//! 2. **Type Mapping**: Maps Rust types to SQL types (via `types::rust_type_to_sql`)
//! 3. **Attribute Parsing**: Extracts ORM attributes like `primary_key`, `unique`, etc.
//! 4. **Code Generation**: Generates the `Model` trait implementation with metadata
//!
//! ## Supported Attributes
//!
//! - `#[orm(primary_key)]` - Marks field as primary key
//! - `#[orm(unique)]` - Adds UNIQUE constraint
//! - `#[orm(index)]` - Creates database index
//! - `#[orm(size = N)]` - Sets VARCHAR size for String fields
//! - `#[orm(create_time)]` - Auto-populate with CURRENT_TIMESTAMP
//! - `#[orm(update_time)]` - Auto-update timestamp (future feature)
//! - `#[orm(foreign_key = "Table::Column")]` - Defines foreign key relationship
//!
//! ## Example
//!
//! ```rust,ignore
//! // Input struct:
//! #[derive(Model)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: Uuid,
//!     #[orm(size = 50, unique)]
//!     username: String,
//!     age: i32,
//! }
//!
//! // Generated implementation:
//! impl Model for User {
//!     fn table_name() -> &'static str { "User" }
//!     fn columns() -> Vec<ColumnInfo> { /* ... */ }
//!     fn active_columns() -> Vec<&'static str> { /* ... */ }
//!     fn to_map(&self) -> HashMap<String, String> { /* ... */ }
//! }
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::types::rust_type_to_sql;

// ============================================================================
// Macro Expansion Function
// ============================================================================

/// Expands the `#[derive(Model)]` macro.
///
/// This function is the main entry point for the derive macro. It takes a
/// parsed struct definition and generates the complete `Model` trait implementation.
///
/// # Arguments
///
/// * `ast` - The parsed struct definition from syn
///
/// # Returns
///
/// A `TokenStream` containing the generated trait implementation
///
/// # Panics
///
/// This function will panic if:
/// - The input is not a struct
/// - The struct doesn't have named fields
/// - Attribute parsing fails
///
/// # Generated Code
///
/// The function generates four methods:
///
/// 1. `table_name()` - Returns the struct name as the table name
/// 2. `columns()` - Returns a `Vec<ColumnInfo>` with metadata for each field
/// 3. `active_columns()` - Returns a `Vec<&'static str>` with field names
/// 4. `to_map()` - Returns a `HashMap<String, String>` serializing the instance
///
/// # Example
///
/// ```rust,ignore
/// // For this struct:
/// #[derive(Model)]
/// struct User {
///     #[orm(primary_key)]
///     id: i32,
///     username: String,
/// }
///
/// // Generates:
/// impl Model for User {
///     fn table_name() -> &'static str {
///         "User"
///     }
///
///     fn columns() -> Vec<ColumnInfo> {
///         vec![
///             ColumnInfo {
///                 name: "id",
///                 sql_type: "INTEGER",
///                 is_primary_key: true,
///                 // ... other fields
///             },
///             ColumnInfo {
///                 name: "username",
///                 sql_type: "TEXT",
///                 is_primary_key: false,
///                 // ... other fields
///             },
///         ]
///     }
///
///     fn active_columns() -> Vec<&'static str> {
///         vec!["id", "username"]
///     }
///
///     fn to_map(&self) -> HashMap<String, String> {
///         let mut map = HashMap::new();
///         map.insert("id".to_string(), self.id.to_string());
///         map.insert("username".to_string(), self.username.to_string());
///         map
///     }
/// }
/// ```
pub fn expand(ast: DeriveInput) -> TokenStream {
    // ========================================================================
    // Extract Struct Information
    // ========================================================================

    let struct_name = &ast.ident;

    // Ensure input is a struct with named fields
    let fields = if let Data::Struct(data) = &ast.data {
        if let Fields::Named(fields) = &data.fields {
            fields
        } else {
            panic!("Model must have named fields");
        }
    } else {
        panic!("Model must be a struct")
    };

    // ========================================================================
    // Generate Column Definitions
    // ========================================================================

    // For each struct field, generate a `ColumnInfo` instance with:
    // - Field name and SQL type
    // - Constraints (primary key, unique, nullable)
    // - Metadata (create_time, update_time, index)
    // - Foreign key relationships
    let column_defs = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        // Map Rust type to SQL type and check if nullable
        let (mut sql_type, is_nullable) = rust_type_to_sql(field_type);

        // Initialize attribute flags with defaults
        let mut is_primary_key = false;
        let mut size = None;
        let mut create_time = false;
        let mut update_time = false;
        let mut unique = false;
        let mut index = false;
        let mut foreign_table_tokens = quote! { None };
        let mut foreign_key_tokens = quote! { None };

        // --------------------------------------------------------------------
        // Parse ORM Attributes
        // --------------------------------------------------------------------
        // Iterate through all attributes on this field looking for #[orm(...)]
        for attr in &f.attrs {
            if attr.path().is_ident("orm") {
                // Parse nested meta items within #[orm(...)]
                attr.parse_nested_meta(|meta| {
                    // --------------------------------------------------------
                    // #[orm(primary_key)]
                    // --------------------------------------------------------
                    if meta.path.is_ident("primary_key") {
                        is_primary_key = true;
                    }

                    // --------------------------------------------------------
                    // #[orm(size = N)]
                    // --------------------------------------------------------
                    // Sets VARCHAR(N) for String fields
                    if meta.path.is_ident("size") {
                        let value: syn::LitInt = meta.value()?.parse()?;
                        size = Some(value.base10_parse::<usize>()?);
                    }

                    // --------------------------------------------------------
                    // #[orm(create_time)]
                    // --------------------------------------------------------
                    // Auto-populate with CURRENT_TIMESTAMP on INSERT
                    if meta.path.is_ident("create_time") {
                        create_time = true;
                    }

                    // --------------------------------------------------------
                    // #[orm(update_time)]
                    // --------------------------------------------------------
                    // Auto-update timestamp on UPDATE (future feature)
                    if meta.path.is_ident("update_time") {
                        update_time = true;
                    }

                    // --------------------------------------------------------
                    // #[orm(unique)]
                    // --------------------------------------------------------
                    // Adds UNIQUE constraint
                    if meta.path.is_ident("unique") {
                        unique = true;
                    }

                    // --------------------------------------------------------
                    // #[orm(index)]
                    // --------------------------------------------------------
                    // Creates database index
                    if meta.path.is_ident("index") {
                        index = true;
                    }

                    // --------------------------------------------------------
                    // #[orm(foreign_key = "Table::Column")]
                    // --------------------------------------------------------
                    // Defines foreign key relationship
                    if meta.path.is_ident("foreign_key") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        let fk_string = value.value();

                        // Parse "Table::Column" format
                        let parts: Vec<&str> = fk_string.split("::").collect();

                        if parts.len() == 2 {
                            let table = parts[0];
                            let col = parts[1];

                            foreign_table_tokens = quote! { Some(#table) };
                            foreign_key_tokens = quote! { Some(#col) };
                        } else {
                            return Err(meta.error("Invalid format for foreign_key. Use 'Table::column'"));
                        }
                    }

                    Ok(())
                })
                .expect("Failed to parse orm attributes");
            }
        }

        // --------------------------------------------------------------------
        // Apply Size Modifier to SQL Type
        // --------------------------------------------------------------------
        // If size attribute is present and type is TEXT, convert to VARCHAR(N)
        if let Some(s) = size
            && sql_type == "TEXT" {
                sql_type = format!("VARCHAR({})", s);
            }

        // --------------------------------------------------------------------
        // Generate ColumnInfo Token Stream
        // --------------------------------------------------------------------
        quote! {
            bottle_orm::ColumnInfo {
                 name: stringify!(#field_name),
                 sql_type: #sql_type,
                 is_primary_key: #is_primary_key,
                 is_nullable: #is_nullable,
                 create_time: #create_time,
                 update_time: #update_time,
                 unique: #unique,
                 index: #index,
                 foreign_table: #foreign_table_tokens,
                 foreign_key: #foreign_key_tokens
            }
        }
    });

    // ========================================================================
    // Generate Active Columns List
    // ========================================================================

    // Extract field identifiers for the `active_columns()` method
    let field_names_iter: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();

    // ========================================================================
    // Generate to_map() Implementation
    // ========================================================================

    // For each field, generate code to insert it into the HashMap.
    // Optional fields (Option<T>) are only inserted if Some.
    let map_inserts = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let (_, is_nullable) = rust_type_to_sql(field_type);

        // Handle Option<T> fields specially - only insert if Some
        if is_nullable {
            return quote! {
                if let Some(val) = &self.#field_name {
                    map.insert(
                        stringify!(#field_name).to_string(),
                        val.to_string()
                    );
                }
            };
        }

        // Regular fields are always inserted
        quote! {
            map.insert(
                stringify!(#field_name).to_string(),
                 self.#field_name.to_string()
            );
        }
    });

    // ========================================================================
    // Generate Complete Model Implementation
    // ========================================================================

    quote! {
        impl bottle_orm::Model for #struct_name {
            /// Returns the table name (struct name as-is).
            ///
            /// This will be converted to snake_case when generating SQL.
            fn table_name() -> &'static str {
                stringify!(#struct_name)
            }

            /// Returns metadata for all columns in the table.
            ///
            /// Each ColumnInfo contains the field's name, SQL type,
            /// constraints, and relationship information.
            fn columns() -> Vec<bottle_orm::ColumnInfo> {
                vec![#(#column_defs),*]
            }

            /// Returns the list of active column names.
            ///
            /// Used for generating SELECT statements and query building.
            fn active_columns() -> Vec<&'static str> {
                vec![#(stringify!(#field_names_iter) ),*]
            }

            /// Converts this instance to a HashMap for INSERT operations.
            ///
            /// All values are converted to strings. Option<T> fields are
            /// only included if they contain Some value.
            fn to_map(&self) -> std::collections::HashMap<String, String> {
                let mut map = std::collections::HashMap::new();
                 #(#map_inserts)*
                  map
            }
        }
    }
}
