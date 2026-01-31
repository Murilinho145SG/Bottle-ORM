//! # Model Derive Macro Implementation
//!
//! This module implements the procedural macro expansion for `#[derive(Model)]`.
//! It analyzes struct fields and their `#[orm(...)]` attributes to automatically
//! generate the `Model` trait implementation.

// ============================================================================
// External Crate Imports
// ============================================================================

use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};
use heck::ToSnakeCase;

// ============================================================================
// Internal Crate Imports
// ============================================================================

use crate::types::rust_type_to_sql;

// ============================================================================
// Helper Functions
// ============================================================================

/// Extracts the inner type `T` from `Option<T>`.
fn get_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

// ============================================================================
// Macro Expansion Function
// ============================================================================

/// Expands the `#[derive(Model)]` macro.
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
        let mut omit = false;
        let mut soft_delete = false;
        let mut foreign_table_tokens = quote! { None };
        let mut foreign_key_tokens = quote! { None };

        // --------------------------------------------------------------------
        // Parse ORM Attributes
        // --------------------------------------------------------------------
        for attr in &f.attrs {
            if attr.path().is_ident("orm") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("primary_key") {
                        is_primary_key = true;
                    }
                    if meta.path.is_ident("size") {
                        let value: syn::LitInt = meta.value()?.parse()?;
                        size = Some(value.base10_parse::<usize>()?);
                    }
                    if meta.path.is_ident("create_time") {
                        create_time = true;
                    }
                    if meta.path.is_ident("update_time") {
                        update_time = true;
                    }
                    if meta.path.is_ident("unique") {
                        unique = true;
                    }
                    if meta.path.is_ident("index") {
                        index = true;
                    }
                    if meta.path.is_ident("foreign_key") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        let fk_string = value.value();
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
                    if meta.path.is_ident("omit") {
                        omit = true;
                    }
                    if meta.path.is_ident("soft_delete") {
                        soft_delete = true;
                    }
                    Ok(())
                })
                .expect("Failed to parse orm attributes");
            }
        }

        if let Some(s) = size
            && sql_type == "TEXT" {
                sql_type = format!("VARCHAR({})", s);
            }

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
                 foreign_key: #foreign_key_tokens,
                 omit: #omit,
                 soft_delete: #soft_delete
            }
        }
    });

    // ========================================================================
    // Generate Active Columns List
    // ========================================================================

    let field_names_iter: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();

    // ========================================================================
    // Generate to_map() Implementation
    // ========================================================================

    let map_inserts = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let (_, is_nullable) = rust_type_to_sql(field_type);

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

        quote! {
            map.insert(
                stringify!(#field_name).to_string(),
                 self.#field_name.to_string()
            );
        }
    });

    // ========================================================================
    // Generate AnyInfo Column Definitions
    // ========================================================================
    let table_name_str = struct_name.to_string().to_snake_case();
    let any_column_defs = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;
        let (sql_type, _) = rust_type_to_sql(field_type);

        quote! {
            bottle_orm::AnyInfo {
                column: stringify!(#field_name),
                sql_type: #sql_type,
                table: #table_name_str,
            }
        }
    });

    // ========================================================================
    // Generate FromRow Implementation (Integrated from FromAnyRow)
    // ========================================================================
    let from_row_logic = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;
        let column_name = field_name.as_ref().unwrap().to_string();
        let alias_name = format!("{}__{}", table_name_str, column_name);

        let (sql_type, is_nullable) = rust_type_to_sql(field_type);

        if sql_type == "TIMESTAMPTZ" || sql_type == "TIMESTAMP" || sql_type == "DATE" || sql_type == "TIME" {
             // For temporal types, we MUST decode as String and parse
             if is_nullable {
                 if let Some(inner_type) = get_inner_type(field_type) {
                     quote! {
                        let #field_name: #field_type = {
                            match row.try_get::<Option<String>, _>(#alias_name).or_else(|_| row.try_get::<Option<String>, _>(#column_name)) {
                                Ok(Some(s)) => {
                                    match s.parse::<#inner_type>() {
                                        Ok(v) => Some(v),
                                        Err(e) => return Err(sqlx::Error::Decode(Box::new(e))),
                                    }
                                },
                                Ok(None) => None,
                                Err(e) => return Err(e)
                            }
                        };
                     }
                 } else {
                     quote! { let #field_name: #field_type = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name))?; }
                 }
             } else {
                 quote! {
                    let #field_name: #field_type = {
                        match row.try_get::<String, _>(#alias_name).or_else(|_| row.try_get::<String, _>(#column_name)) {
                            Ok(s) => s.parse().map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                            Err(e) => return Err(e)
                        }
                    };
                 }
             }
        } else if sql_type == "UUID" {
             // For UUID, we try string parse first
             if is_nullable {
                 if let Some(inner_type) = get_inner_type(field_type) {
                     quote! {
                        let #field_name: #field_type = {
                            match row.try_get::<Option<String>, _>(#alias_name).or_else(|_| row.try_get::<Option<String>, _>(#column_name)) {
                                Ok(Some(s)) => {
                                    match s.parse::<#inner_type>() {
                                        Ok(v) => Some(v),
                                        Err(e) => return Err(sqlx::Error::Decode(Box::new(e))),
                                    }
                                },
                                Ok(None) => None,
                                Err(e) => return Err(e)
                            }
                        };
                     }
                 } else {
                     quote! { let #field_name: #field_type = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name))?; }
                 }
             } else {
                 quote! {
                    let #field_name: #field_type = {
                        match row.try_get::<String, _>(#alias_name).or_else(|_| row.try_get::<String, _>(#column_name)) {
                            Ok(s) => s.parse().map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                            Err(e) => return Err(e)
                        }
                    };
                 }
             }
        } else {
            quote! {
                let #field_name: #field_type = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name))?;
            }
        }
    });

    let field_names_construct = fields.named.iter().map(|f| &f.ident);
    // We need to clone the logic for the second implementation as iterator is consumed
    let from_row_logic_clone = from_row_logic.clone();
    let field_names_construct_clone = field_names_construct.clone();

    // ========================================================================
    // Generate Fields Module for Autocomplete
    // ========================================================================

    let module_name = format_ident!("{}_fields", struct_name.to_string().to_snake_case());
    let field_constants = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let const_name = format_ident!("{}", field_name.as_ref().unwrap().to_string().to_uppercase());
        let name_str = field_name.as_ref().unwrap().to_string();
        quote! {
            pub const #const_name: &'static str = #name_str;
        }
    });

    // ========================================================================
    // Generate Complete Model & AnyImpl & FromRow Implementation
    // ========================================================================

    quote! {
        /// Auto-generated field constants for autocomplete support.
        ///
        /// Use these constants with `filter()`, `select()`, `omit()`, and other
        /// QueryBuilder methods for IDE autocomplete.
        ///
        /// # Example
        /// ```rust,ignore
        /// use crate::user_fields;
        /// db.model::<User>()
        ///     .filter(user_fields::AGE, ">=", 18)
        ///     .omit(user_fields::PASSWORD)
        ///     .scan()
        ///     .await?;
        /// ```
        pub mod #module_name {
            #(#field_constants)*
        }

        impl bottle_orm::Model for #struct_name {
            fn table_name() -> &'static str {
                stringify!(#struct_name)
            }

            fn columns() -> Vec<bottle_orm::ColumnInfo> {
                vec![#(#column_defs),*]
            }

            fn active_columns() -> Vec<&'static str> {
                vec![#(stringify!(#field_names_iter) ),*]
            }

            fn to_map(&self) -> std::collections::HashMap<String, String> {
                let mut map = std::collections::HashMap::new();
                 #(#map_inserts)*
                  map
            }
        }

        impl bottle_orm::AnyImpl for #struct_name {
            fn columns() -> Vec<bottle_orm::AnyInfo> {
                vec![#(#any_column_defs),*]
            }

            fn to_map(&self) -> std::collections::HashMap<String, String> {
                bottle_orm::Model::to_map(self)
            }
        }

        impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for #struct_name {
             fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
                 use sqlx::Row;
                 #(#from_row_logic)*

                 Ok(#struct_name {
                     #(#field_names_construct),*
                 })
             }
        }

        impl bottle_orm::any_struct::FromAnyRow for #struct_name {
             fn from_any_row(row: &sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
                 use sqlx::Row;
                 #(#from_row_logic_clone)*

                 Ok(#struct_name {
                     #(#field_names_construct_clone),*
                 })
             }
        }
    }
}
