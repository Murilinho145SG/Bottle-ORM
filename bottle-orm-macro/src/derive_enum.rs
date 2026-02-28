//! # Enum Derive Macro Implementation
//!
//! This module implements the procedural macro expansion for `#[derive(BottleEnum)]`.
//! It automatically generates `Display` and `FromStr` implementations for enums,
//! using the variant names as the string representation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput};
use heck::ToSnakeCase;

/// Expands the `#[derive(BottleEnum)]` macro.
pub fn expand(ast: DeriveInput) -> TokenStream {
    let name = &ast.ident;
    
    // Ensure input is an enum
    let variants = match &ast.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("BottleEnum can only be derived for enums"),
    };

    // Generate Display arms: Self::Variant => "variant"
    let display_arms = variants.iter().map(|variant| {
        let variant_ident = &variant.ident;
        let variant_name_snake = variant_ident.to_string().to_snake_case();
        quote! {
            Self::#variant_ident => write!(f, #variant_name_snake),
        }
    });

    // Generate FromStr arms: "variant" => Ok(Self::Variant)
    let from_str_arms = variants.iter().map(|variant| {
        let variant_ident = &variant.ident;
        let variant_name_snake = variant_ident.to_string().to_snake_case();
        quote! {
            #variant_name_snake => Ok(Self::#variant_ident),
        }
    });

    // Output the generated implementations
    quote! {
        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }

        impl std::str::FromStr for #name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    #(#from_str_arms)*
                    _ => Err(format!("Unknown variant: {}", s)),
                }
            }
        }
    }
}
