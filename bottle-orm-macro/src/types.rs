//! # Type Mapping Module
//!
//! This module handles the conversion between Rust types and their corresponding SQL types.
//! It supports standard Rust primitives, chrono datetime types, and UUID types (versions 1-7).
//!
//! ## Supported Type Mappings
//!
//! ### Primitive Types
//! - `i32` → `INTEGER`
//! - `i64` → `BIGINT`
//! - `String` → `TEXT`
//! - `bool` → `BOOLEAN`
//! - `f64` → `DOUBLE PRECISION`
//!
//! ### Date/Time Types (chrono)
//! - `DateTime<Utc>` → `TIMESTAMPTZ`
//! - `NaiveDateTime` → `TIMESTAMP`
//! - `NaiveDate` → `DATE`
//! - `NaiveTime` → `TIME`
//!
//! ### UUID Types
//! - `Uuid` → `UUID` (supports all versions 1-7)
//!
//! ### Nullable Types
//! - `Option<T>` → SQL type of `T` with `NULL` allowed
//!
//! ## Example
//!
//! ```rust,ignore
//! use uuid::Uuid;
//! use chrono::{DateTime, Utc};
//!
//! #[derive(Model)]
//! struct User {
//!     #[orm(primary_key)]
//!     id: Uuid,              // Maps to UUID
//!     username: String,       // Maps to TEXT
//!     age: Option<i32>,      // Maps to INTEGER (nullable)
//!     created_at: DateTime<Utc>, // Maps to TIMESTAMPTZ
//! }
//! ```

use syn::{GenericArgument, PathArguments, Type};

/// Maps Rust types to their corresponding SQL types.
///
/// This function analyzes a Rust type and returns the appropriate SQL type string
/// along with a boolean indicating whether the type is nullable (wrapped in `Option`).
///
/// # Arguments
///
/// * `ty` - A reference to the Rust type to be mapped
///
/// # Returns
///
/// A tuple containing:
/// 1. `String` - The SQL type string (e.g., "TEXT", "INTEGER", "UUID")
/// 2. `bool` - `true` if the type is wrapped in `Option<T>`, `false` otherwise
///
/// # Type Mapping Logic
///
/// The function performs the following checks in order:
///
/// 1. **Option<T> Detection**: If the type is `Option<T>`, it recursively maps
///    the inner type `T` and marks it as nullable.
///
/// 2. **Primitive Types**: Maps standard Rust primitives to their SQL equivalents:
///    - Integer types (`i32`, `i64`) → `INTEGER`, `BIGINT`
///    - String type → `TEXT`
///    - Boolean type → `BOOLEAN`
///    - Floating-point type (`f64`) → `DOUBLE PRECISION`
///
/// 3. **UUID Types**: Maps `Uuid` type to `UUID` SQL type.
///    Supports all UUID versions (1-7) seamlessly.
///
/// 4. **Date/Time Types**: Maps chrono types to appropriate SQL temporal types:
///    - `DateTime` (with timezone) → `TIMESTAMPTZ`
///    - `NaiveDateTime` (without timezone) → `TIMESTAMP`
///    - `NaiveDate` → `DATE`
///    - `NaiveTime` → `TIME`
///
/// 5. **Fallback**: Any unrecognized type defaults to `TEXT`.
///
/// # Examples
///
/// ```rust,ignore
/// // Non-nullable integer
/// let (sql_type, nullable) = rust_type_to_sql(&parse_quote!(i32));
/// assert_eq!(sql_type, "INTEGER");
/// assert_eq!(nullable, false);
///
/// // Nullable string
/// let (sql_type, nullable) = rust_type_to_sql(&parse_quote!(Option<String>));
/// assert_eq!(sql_type, "TEXT");
/// assert_eq!(nullable, true);
///
/// // UUID type
/// let (sql_type, nullable) = rust_type_to_sql(&parse_quote!(Uuid));
/// assert_eq!(sql_type, "UUID");
/// assert_eq!(nullable, false);
///
/// // Nullable UUID
/// let (sql_type, nullable) = rust_type_to_sql(&parse_quote!(Option<Uuid>));
/// assert_eq!(sql_type, "UUID");
/// assert_eq!(nullable, true);
/// ```
pub fn rust_type_to_sql(ty: &Type) -> (String, bool) {
    // Check if the type is a path type (e.g., String, i32, Option<T>, Uuid)
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();

            // ================================================================
            // Handle Option<T> for nullable columns
            // ================================================================
            // When a field is wrapped in Option<T>, we need to:
            // 1. Extract the inner type T
            // 2. Map T to its SQL type
            // 3. Mark the column as nullable
            if type_name == "Option"
                && let PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        // Recursively map the inner type and force nullable = true
                        let (inner_sql_type, _ignored_nullable) = rust_type_to_sql(inner_ty);
                        return (inner_sql_type, true);
                    }

            // ================================================================
            // Map non-nullable types to their SQL equivalents
            // ================================================================
            match type_name.as_str() {
                // ------------------------------------------------------------
                // Integer Types
                // ------------------------------------------------------------
                // i32 → INTEGER (4-byte signed integer)
                // Commonly used for primary keys and numeric IDs
                "i32" => ("INTEGER".to_string(), false),

                // i64 → BIGINT (8-byte signed integer)
                // Used for large numeric values and timestamps
                "i64" => ("BIGINT".to_string(), false),

                // ------------------------------------------------------------
                // Text Types
                // ------------------------------------------------------------
                // String → TEXT (variable-length text)
                // Can be constrained with #[orm(size = N)] to VARCHAR(N)
                "String" => ("TEXT".to_string(), false),

                // ------------------------------------------------------------
                // Boolean Type
                // ------------------------------------------------------------
                // bool → BOOLEAN (true/false values)
                "bool" => ("BOOLEAN".to_string(), false),

                // ------------------------------------------------------------
                // Floating-Point Types
                // ------------------------------------------------------------
                // f64 → DOUBLE PRECISION (8-byte floating-point)
                // Used for decimal numbers requiring high precision
                "f64" => ("DOUBLE PRECISION".to_string(), false),

                // ------------------------------------------------------------
                // UUID Types (Versions 1-7)
                // ------------------------------------------------------------
                // Uuid → UUID (Universally Unique Identifier)
                //
                // Supports all UUID versions:
                // - Version 1: Time-based UUID (MAC address + timestamp)
                // - Version 3: Name-based UUID (MD5 hash)
                // - Version 4: Random UUID (most common)
                // - Version 5: Name-based UUID (SHA-1 hash)
                // - Version 6: Reordered time-based UUID (better database indexing)
                // - Version 7: Unix timestamp-based UUID (sortable, recommended)
                //
                // Example usage:
                // ```rust,ignore
                // use uuid::Uuid;
                //
                // #[derive(Model)]
                // struct User {
                //     #[orm(primary_key)]
                //     id: Uuid,  // UUID v4 recommended for primary keys
                //     // Or use Option<Uuid> for nullable UUID fields
                //     external_id: Option<Uuid>,
                // }
                // ```
                "Uuid" => ("UUID".to_string(), false),

                // ------------------------------------------------------------
                // Date/Time Types (chrono)
                // ------------------------------------------------------------
                // DateTime → TIMESTAMPTZ (timestamp with time zone)
                // Stores absolute point in time with timezone information
                // Recommended for created_at, updated_at fields
                "DateTime" => ("TIMESTAMPTZ".to_string(), false),

                // NaiveDateTime → TIMESTAMP (timestamp without time zone)
                // Stores date and time without timezone information
                "NaiveDateTime" => ("TIMESTAMP".to_string(), false),

                // NaiveDate → DATE (calendar date)
                // Stores only the date portion (year, month, day)
                "NaiveDate" => ("DATE".to_string(), false),

                // NaiveTime → TIME (time of day)
                // Stores only the time portion (hours, minutes, seconds)
                "NaiveTime" => ("TIME".to_string(), false),

                // ------------------------------------------------------------
                // Fallback for Unknown Types
                // ------------------------------------------------------------
                // Any unrecognized type defaults to TEXT
                // This provides a safe fallback for custom types that
                // implement Display/ToString traits
                _ => ("TEXT".to_string(), false),
            }
        } else {
            // Path has no segments (shouldn't happen in practice)
            ("TEXT".to_string(), false)
        }
    } else {
        // Not a path type (e.g., reference, pointer, array)
        // Default to TEXT for safety
        ("TEXT".to_string(), false)
    }
}
