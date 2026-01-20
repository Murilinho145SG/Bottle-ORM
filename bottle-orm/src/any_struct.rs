//! # Any Structure Support
//!
//! This module defines traits and structures to support mapping arbitrary database rows
//! (specifically `AnyRow`) to Rust structs. It provides metadata about columns
//! to facilitate dynamic query construction and result mapping.

#[derive(Debug, Clone)]
/// Contains metadata about a database column.
pub struct AnyInfo {
    /// The name of the column.
	pub column: &'static str,
    /// The SQL type of the column.
	pub sql_type: &'static str
}

/// A trait for types that can be mapped from an `AnyRow` and provide column metadata.
///
/// This trait is typically implemented via the `FromAnyRow` derive macro.
pub trait AnyImpl {
    /// Returns a vector of `AnyInfo` describing the columns associated with this type.
	fn columns() -> Vec<AnyInfo>;
}

macro_rules! impl_any_primitive {
    ($($t:ty),*) => {
        $(
            impl AnyImpl for $t {
                fn columns() -> Vec<AnyInfo> {
                    Vec::new()
                }
            }
        )*
    };
}

impl_any_primitive!(
    bool,
    i8, i16, i32, i64, isize,
    u8, u16, u32, u64, usize,
    f32, f64,
    String
);

impl AnyImpl for uuid::Uuid {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
}

impl AnyImpl for chrono::NaiveDateTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
}

impl AnyImpl for chrono::NaiveDate {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
}

impl AnyImpl for chrono::NaiveTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
}

impl AnyImpl for chrono::DateTime<chrono::Utc> {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
}

impl<T: AnyImpl> AnyImpl for Option<T> {
    fn columns() -> Vec<AnyInfo> {
        T::columns()
    }
}

macro_rules! impl_any_tuple {
    ($($T:ident),+) => {
        impl<$($T: AnyImpl),+> AnyImpl for ($($T,)+) {
            fn columns() -> Vec<AnyInfo> {
                Vec::new()
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
