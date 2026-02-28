//! # Derive AnyRow Module
//!
//! This module implements the `FromAnyRow` derive macro.
//! It generates the necessary code to convert a database row (AnyRow) into a Rust struct,
//! with special handling for specific types like `DateTime`.

use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

use crate::types::rust_type_to_sql;

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
    let table_name = struct_name.to_string().to_snake_case();

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
        let alias_name = format!("{}__{}", table_name, column_name);
        
        let mut is_enum = false;
        for attr in &f.attrs {
            if attr.path().is_ident("orm") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("enum") {
                        is_enum = true;
                    }
                    Ok(())
                });
            }
        }

        if is_enum {
            let (_, is_nullable) = rust_type_to_sql(field_type);
            if is_nullable {
                if let Some(inner_type) = get_inner_type(field_type) {
                    quote! {
                        let #field_name: #field_type = {
                            match row.try_get::<Option<String>, _>(#alias_name).or_else(|_| row.try_get::<Option<String>, _>(#column_name)) {
                                Ok(Some(s)) => {
                                    match s.parse::<#inner_type>() {
                                        Ok(v) => Some(v),
                                        Err(e) => return Err(sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to parse enum: {}", e))))),
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
                        match row.try_get::<String, _>(#alias_name).or_else(|_| row.try_get(#column_name)) {
                            Ok(s) => s.parse().map_err(|e| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to parse enum: {}", e)))))?,
                            Err(e) => return Err(e)
                        }
                    };
                }
            }
        } else if is_datetime(field_type) {
            quote! {
                let #field_name: #field_type = {
                     let s: String = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name)).map_err(|e| sqlx::Error::ColumnDecode {
                        index: #column_name.to_string(),
                        source: Box::new(e)
                    })?;

                     s.parse::<chrono::DateTime<chrono::Utc>>().map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                };
            }
        } else if is_uuid(field_type) {
            // Special handling for Uuid fields: parse from string
            // UUIDs are typically returned as strings from the database when using AnyRow
            quote! {
                let #field_name: #field_type = {
                     let s: String = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name)).map_err(|e| sqlx::Error::ColumnDecode {
                        index: #column_name.to_string(),
                        source: Box::new(e)
                    })?;

                     s.parse::<uuid::Uuid>().map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                };
            }
        } else {
            // Standard handling for other types
            quote! {
                let #field_name: #field_type = row.try_get(#alias_name).or_else(|_| row.try_get(#column_name))?;
            }
        }
    });

    // Generate logic for extracting each field from the row by index (positional)
    let ext_logic_positional = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let mut is_enum = false;
        for attr in &f.attrs {
            if attr.path().is_ident("orm") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("enum") {
                        is_enum = true;
                    }
                    Ok(())
                });
            }
        }

        // Special handling for Enum, DateTime fields: parse from string
        if is_enum || is_datetime(field_type) || is_uuid(field_type) {
            let (_, is_nullable) = rust_type_to_sql(field_type);
            if is_nullable {
                if let Some(inner_type) = get_inner_type(field_type) {
                    quote! {
                        let #field_name: #field_type = {
                            let s: Option<String> = row.try_get(*index).map_err(|e| sqlx::Error::ColumnDecode {
                                index: index.to_string(),
                                source: Box::new(e)
                            })?;
                            *index += 1;
                            match s {
                                Some(s_val) => Some(s_val.parse::<#inner_type>().map_err(|e| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to parse enum: {}", e)))))),
                                None => None,
                            }
                        };
                    }
                } else {
                    quote! {
                        let #field_name: #field_type = row.try_get(*index)?;
                        *index += 1;
                    }
                }
            } else {
                quote! {
                    let #field_name: #field_type = {
                         let s: String = row.try_get(*index).map_err(|e| sqlx::Error::ColumnDecode {
                            index: index.to_string(),
                            source: Box::new(e)
                        })?;
                         *index += 1;
                         s.parse().map_err(|e| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to parse enum: {}", e)))))?
                    };
                }
            }
        } else {
            quote! {
                let #field_name: #field_type = row.try_get(*index)?;
                *index += 1;
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
                sql_type: #sql_type,
                table: #table_name
            }
        }
    });

    let field_names = fields.iter().map(|f| &f.ident);
    let field_names_clone = field_names.clone();
    let field_names_positional = field_names.clone();
    let ext_logic_clone = ext_logic.clone();

    // Generate to_map implementation
    let map_inserts = fields.iter().map(|f| {
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

         impl ::bottle_orm::any_struct::FromAnyRow for #struct_name {
             fn from_any_row(row: &sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
                 use sqlx::Row;
                #(#ext_logic_clone)*

                Ok(#struct_name {
                     #(#field_names_clone),*
                 })
               }

             fn from_any_row_at(row: &sqlx::any::AnyRow, index: &mut usize) -> Result<Self, sqlx::Error> {
                 use sqlx::Row;
                #(#ext_logic_positional)*

                Ok(#struct_name {
                     #(#field_names_positional),*
                 })
               }
         }

         impl bottle_orm::AnyImpl for #struct_name {
             fn columns() -> Vec<bottle_orm::AnyInfo> {
                  vec![#(#col_query),*]
              }

             fn to_map(&self) -> std::collections::HashMap<String, String> {
                let mut map = std::collections::HashMap::new();
                #(#map_inserts)*
                map
             }
         }
    }
}

/// Checks if the given type is a DateTime type.
fn is_datetime(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
            && segment.ident == "DateTime" {
                return true;
            }
    false
}

/// Checks if the given type is a Uuid type.
fn is_uuid(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
            && segment.ident == "Uuid" {
                return true;
            }
    false
}
