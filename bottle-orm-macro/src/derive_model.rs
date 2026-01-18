use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};
use crate::types::rust_type_to_sql;

/// Expands the `#[derive(Model)]` macro.
///
/// This function parses the struct fields and `#[orm(...)]` attributes to generate:
/// 1. `ColumnInfo` metadata for each field.
/// 2. The `impl Model` block with methods like `table_name`, `columns`, and `to_map`.
pub fn expand(ast: DeriveInput) -> TokenStream {
    let struct_name = &ast.ident;

    let fields = if let Data::Struct(data) = &ast.data {
        if let Fields::Named(fields) = &data.fields {
            fields
        } else {
            panic!("Model must have named fields");
        }
    } else {
        panic!("Model must be a struct")
    };

    let column_defs = fields.named.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let (mut sql_type, is_nullable) = rust_type_to_sql(field_type);
        let mut is_primary_key = false;
        let mut size = None;
        let mut create_time = false;
        let mut update_time = false;
        let mut unique = false;
        let mut index = false;
        let mut foreign_table_tokens = quote! { None };
        let mut foreign_key_tokens = quote! { None };

        // Parse attributes #[orm(...)]
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

                    Ok(())
                })
                .expect("Failed to parse orm attributes");
            }
        }

        if let Some(s) = size {
            if sql_type == "TEXT" {
                sql_type = format!("VARCHAR({})", s);
            }
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
                 foreign_key: #foreign_key_tokens
            }
        }
    });

    let field_names_iter: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
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

    quote! {
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
    }
}
