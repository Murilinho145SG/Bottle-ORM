//! # Any Structure Support
//!
//! This module defines traits and structures to support mapping arbitrary database rows
//! (specifically `AnyRow`) to Rust structs. It provides metadata about columns
//! to facilitate dynamic query construction and result mapping.
//!
//! ## Features
//!
//! - **Dynamic Mapping**: Supports mapping `AnyRow` to struct fields
//! - **Metadata Reflection**: Provides column names and types at runtime
//! - **Extensible**: Can be implemented for custom types
//!
//! ## Example
//!
//! ```rust,ignore
//! use bottle_orm::{AnyImpl, AnyInfo};
//!
//! struct MyStruct {
//!     id: i32,
//!     name: String,
//! }
//!
//! impl AnyImpl for MyStruct {
//!     fn columns() -> Vec<AnyInfo> {
//!         vec![
//!             AnyInfo { column: "id", sql_type: "INTEGER" },
//!             AnyInfo { column: "name", sql_type: "TEXT" },
//!         ]
//!     }
//! }
//! ```

use std::collections::HashMap;

// ============================================================================
// AnyInfo Structure
// ============================================================================

/// Contains metadata about a database column.
///
/// This struct is used to describe the schema of a model or query result,
/// providing the necessary information for the query builder to construct
/// valid SQL statements.
#[derive(Debug, Clone)]
pub struct AnyInfo {
    /// The name of the column in the database.
    pub column: &'static str,

    /// The SQL type of the column (e.g., "INTEGER", "TEXT", "UUID").
    pub sql_type: &'static str,
}

// ============================================================================
// AnyImpl Trait
// ============================================================================

/// A trait for types that can be mapped from an `AnyRow` and provide column metadata.
///
/// This trait is the backbone of the ORM's reflection capabilities. It allows the
/// system to know which columns correspond to which fields in a Rust struct.
///
/// This trait is typically implemented automatically via the `FromAnyRow` derive macro,
/// but can be implemented manually for custom scenarios.
pub trait AnyImpl {
    /// Returns a vector of `AnyInfo` describing the columns associated with this type.
    fn columns() -> Vec<AnyInfo>;

    /// Converts this instance to a HashMap for dynamic query building.
    fn to_map(&self) -> HashMap<String, String>;
}

// ============================================================================
// Primitive Implementations
// ============================================================================

macro_rules! impl_any_primitive {
    ($($t:ty),*) => {
        $(
            impl AnyImpl for $t {
                fn columns() -> Vec<AnyInfo> {
                    Vec::new()
                }

                fn to_map(&self) -> HashMap<String, String> {
                    HashMap::new()
                }
            }
        )*
    };
}

impl_any_primitive!(bool, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, String);

// ============================================================================
// External Type Implementations
// ============================================================================

impl AnyImpl for uuid::Uuid {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }

    fn to_map(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl AnyImpl for chrono::NaiveDateTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }

    fn to_map(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl AnyImpl for chrono::NaiveDate {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }

    fn to_map(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl AnyImpl for chrono::NaiveTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }

    fn to_map(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl AnyImpl for chrono::DateTime<chrono::Utc> {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }

    fn to_map(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

// ============================================================================
// Option Implementation
// ============================================================================

impl<T: AnyImpl> AnyImpl for Option<T> {
    fn columns() -> Vec<AnyInfo> {
        T::columns()
    }

    fn to_map(&self) -> HashMap<String, String> {
        match self {
            Some(v) => v.to_map(),
            None => HashMap::new(),
        }
    }
}

// ============================================================================
// Tuple Implementations
// ============================================================================

macro_rules! impl_any_tuple {
    ($($T:ident),+) => {
        impl<$($T: AnyImpl),+> AnyImpl for ($($T,)+) {
            fn columns() -> Vec<AnyInfo> {
                Vec::new()
            }

            fn to_map(&self) -> HashMap<String, String> {
                HashMap::new()
            }
        }
    };
}

impl_any_tuple!(T1);
impl_any_tuple!(T1, T2);
impl_any_tuple!(T1, T2, T3);
impl_any_tuple!(T1, T2, T3, T4);
impl_any_tuple!(T1, T2, T3, T4, T5);
impl_any_tuple!(T1, T2, T3, T4, T5, T6);
impl_any_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_any_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
