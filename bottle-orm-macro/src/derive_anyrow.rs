//! # Derive AnyRow Module
//!
//! This module implements the `FromAnyRow` derive macro.
//! It generates the necessary code to convert a database row (AnyRow) into a Rust struct,
//! with special handling for specific types like `DateTime`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type};

use crate::types::rust_type_to_sql;

/// Expands the `FromAnyRow` derive macro.
///
/// This function generates:
/// 1. An implementation of `sqlx::FromRow` for the struct.
/// 2. An implementation of the `AnyImpl` trait.
///
/// It handles field mapping and specifically manages `DateTime` fields by parsing them
/// from strings (which facilitates compatibility across different database drivers when
/// using `to_json` or similar casts).
pub fn expand(input: DeriveInput) -> TokenStream {
    let struct_name = input.ident;

    // Extract fields from the struct
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            _ => panic!("Only structs with fields named supported"),
        },
        _ => panic!("Only structs supported"),
    };

    // Generate logic for extracting each field from the row
    let ext_logic = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;
        let column_name = field_name.as_ref().unwrap().to_string();
        
        // Special handling for DateTime fields: parse from string
        if is_datetime(field_type) {
            dbg!(&column_name);
            dbg!(&field_type);
            quote! {
                let #field_name: #field_type = {
                     let s: String = row.try_get(#column_name).map_err(|e| sqlx::Error::ColumnDecode {
                        index: #column_name.to_string(),
                        source: Box::new(e)
                    })?;

                     s.parse::<chrono::DateTime<chrono::Utc>>().map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                };
            }
        } else {
            // Standard handling for other types
            quote! {
                let #field_name: #field_type = row.try_get(#column_name)?;
            }
        }
    });

    // Generate column metadata for AnyImpl
    let col_query = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let (sql_type, _) = rust_type_to_sql(field_type);

        quote! {
            bottle_orm::AnyInfo {
                column: stringify!(#field_name),
                sql_type: #sql_type
            }
        }
    });

    let field_names = fields.iter().map(|f| &f.ident);

    // Combine everything into the final implementation
    quote! {
        impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for #struct_name {
             fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
                 use sqlx::Row;
                #(#ext_logic)*

                Ok(#struct_name {
                     #(#field_names),*
                 })
               }
         }

         impl bottle_orm::AnyImpl for #struct_name {
             fn columns() -> Vec<bottle_orm::AnyInfo> {
                  vec![#(#col_query),*]
              }
         }
    }
}

/// Checks if the given type is a DateTime type.
fn is_datetime(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "DateTime" {
                return true;
            }
        }
    }
    false
}
