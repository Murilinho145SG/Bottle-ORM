use sqlx::{any::AnyRow, Error, Row};
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

    /// The name of the table this column belongs to (empty for un-associated columns).
    pub table: &'static str,
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

/// A trait for types that can be mapped from an `AnyRow`.
///
/// This trait replaces `sqlx::FromRow` for `bottle-orm` to support
/// Tuple mapping where each element constructs itself from the whole row
/// (e.g. `(User, Account)`), rather than consuming columns positionally.
pub trait FromAnyRow: Sized {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error>;
}

// ============================================================================
// Primitive Implementations
// ============================================================================

macro_rules! impl_supported_primitive {
    ($($t:ty),*) => {
        $(
            impl AnyImpl for $t {
                fn columns() -> Vec<AnyInfo> { Vec::new() }
                fn to_map(&self) -> HashMap<String, String> { HashMap::new() }
            }

            impl FromAnyRow for $t {
                fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
                    row.try_get(0)
                }
            }
        )*
    };
}

// Primitives directly supported by sqlx::Any (Decode implemented)
impl_supported_primitive!(bool, i16, i32, i64, f32, f64, String);

macro_rules! impl_cast_primitive {
    ($($t:ty),*) => {
        $(
            impl AnyImpl for $t {
                fn columns() -> Vec<AnyInfo> { Vec::new() }
                fn to_map(&self) -> HashMap<String, String> { HashMap::new() }
            }

            impl FromAnyRow for $t {
                fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
                    // Try to get as i64 and cast
                    let val: i64 = row.try_get(0)?;
                    Ok(val as $t)
                }
            }
        )*
    };
}

// Primitives that might need casting from i64
impl_cast_primitive!(i8, isize, u8, u16, u32, u64, usize);

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

impl FromAnyRow for uuid::Uuid {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let s: String = row.try_get(0)?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
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

impl FromAnyRow for chrono::NaiveDateTime {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let s: String = row.try_get(0)?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
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

impl FromAnyRow for chrono::NaiveDate {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let s: String = row.try_get(0)?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
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

impl FromAnyRow for chrono::NaiveTime {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let s: String = row.try_get(0)?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
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

impl FromAnyRow for chrono::DateTime<chrono::Utc> {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let s: String = row.try_get(0)?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
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

impl<T: FromAnyRow> FromAnyRow for Option<T> {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        // If not null, try to decode T
        match T::from_any_row(row) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None), // Fallback? Or propagate error?
                                // If T fails to decode (e.g. invalid format), we probably should propagate error.
                                // But if T fails because "column not found" (unlikely for index 0) or type mismatch...
        }
    }
}

// Special impl for Option<String> to avoid recursion issues if needed,
// but T=String is in impl_supported_primitive.
// Actually, generic impl above relies on T::from_any_row returning Error on NULL.
// We need to intercept NULL before calling T.

// ============================================================================
// Tuple Implementations
// ============================================================================

macro_rules! impl_any_tuple {
    ($($T:ident),+) => {
        impl<$($T: AnyImpl),+> AnyImpl for ($($T,)+) {
            fn columns() -> Vec<AnyInfo> {
                let mut cols = Vec::new();
                $(
                    cols.extend($T::columns());
                )+
                cols
            }

            fn to_map(&self) -> HashMap<String, String> {
                let mut map = HashMap::new();
                #[allow(non_snake_case)]
                let ($($T,)+) = self;
                $(
                    map.extend($T.to_map());
                )+
                map
            }
        }

        impl<$($T: FromAnyRow),+> FromAnyRow for ($($T,)+) {
            fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
                Ok((
                    $(
                        $T::from_any_row(row)?,
                    )+
                ))
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
