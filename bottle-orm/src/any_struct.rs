use sqlx::{any::AnyRow, Error, Row};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

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

/// A generic placeholder struct that implements AnyImpl.
/// Used in closures where a concrete model type is required but any model should be accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnyImplStruct {}

impl AnyImpl for AnyImplStruct {
    fn columns() -> Vec<AnyInfo> { Vec::new() }
    fn to_map(&self) -> HashMap<String, Option<String>> { HashMap::new() }
}

impl FromAnyRow for AnyImplStruct {
    fn from_any_row(_row: &AnyRow) -> Result<Self, Error> { Ok(AnyImplStruct {}) }
    fn from_any_row_at(_row: &AnyRow, _index: &mut usize) -> Result<Self, Error> { Ok(AnyImplStruct {}) }
}

impl crate::model::Model for AnyImplStruct {
    fn table_name() -> &'static str { "" }
    fn columns() -> Vec<crate::model::ColumnInfo> { Vec::new() }
    fn column_names() -> Vec<String> { Vec::new() }
    fn active_columns() -> Vec<&'static str> { Vec::new() }
    fn relations() -> Vec<crate::model::RelationInfo> { Vec::new() }
    fn load_relations<'a>(
        _relation_name: &'a str,
        _models: &'a mut [Self],
        _tx: &'a dyn crate::database::Connection,
        _query_modifier: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
    ) -> futures::future::BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move { Ok(()) })
    }
    fn to_map(&self) -> HashMap<String, Option<String>> { HashMap::new() }
}

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
    fn to_map(&self) -> HashMap<String, Option<String>>;
}

/// A trait for types that can be mapped from an `AnyRow`.
pub trait FromAnyRow: Sized {
    /// Constructs the type from the whole row.
    fn from_any_row(row: &AnyRow) -> Result<Self, Error>;

    /// Constructs the type from the row starting at the given index,
    /// incrementing the index for each column consumed.
    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error>;
}

// ============================================================================
// Primitive Implementations
// ============================================================================

macro_rules! impl_supported_primitive {
    ($($t:ty),*) => {
        $(
            impl AnyImpl for $t {
                fn columns() -> Vec<AnyInfo> { Vec::new() }
                fn to_map(&self) -> HashMap<String, Option<String>> { HashMap::new() }
            }

            impl FromAnyRow for $t {
                fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
                    row.try_get(0).map_err(|e| Error::Decode(Box::new(e)))
                }

                fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
                    if *index >= row.len() {
                        return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
                    }
                    let res = row.try_get(*index);
                    *index += 1;
                    res.map_err(|e| Error::Decode(Box::new(e)))
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
                fn to_map(&self) -> HashMap<String, Option<String>> { HashMap::new() }
            }

            impl FromAnyRow for $t {
                fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
                    let val: i64 = row.try_get(0).map_err(|e| Error::Decode(Box::new(e)))?;
                    <$t>::try_from(val).map_err(|e| Error::Decode(Box::new(e)))
                }

                fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
                    if *index >= row.len() {
                        return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
                    }
                    let res = row.try_get::<i64, _>(*index);
                    *index += 1;
                    let val = res.map_err(|e| Error::Decode(Box::new(e)))?;
                    <$t>::try_from(val).map_err(|e| Error::Decode(Box::new(e)))
                }
            }
        )*
    };
}

// Primitives that might need casting from i64
impl_cast_primitive!(i8, isize, u8, u16, u32, u64, usize);

// ============================================================================
// Array and JSON Implementations
// ============================================================================

impl<T> AnyImpl for Vec<T>
where
    T: AnyImpl + Serialize + for<'de> Deserialize<'de>,
{
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        let mut map = HashMap::new();
        if let Ok(json) = serde_json::to_string(self) {
            map.insert("".to_string(), Some(json));
        }
        map
    }
}

impl<T> FromAnyRow for Vec<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send,
{
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        *index += 1;
        let s = res.map_err(|e| Error::Decode(Box::new(e)))?;
        serde_json::from_str(&s).map_err(|e| Error::Decode(Box::new(e)))
    }
}

impl AnyImpl for serde_json::Value {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        let mut map = HashMap::new();
        map.insert("".to_string(), Some(self.to_string()));
        map
    }
}

impl FromAnyRow for serde_json::Value {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        *index += 1;
        let s = res.map_err(|e| Error::Decode(Box::new(e)))?;
        serde_json::from_str(&s).map_err(|e| Error::Decode(Box::new(e)))
    }
}

// ============================================================================
// External Type Implementations
// ============================================================================

impl AnyImpl for uuid::Uuid {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        HashMap::new()
    }
}

impl FromAnyRow for uuid::Uuid {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        *index += 1;
        let s = res.map_err(|e| Error::Decode(Box::new(e)))?;
        s.parse().map_err(|e| Error::Decode(Box::new(e)))
    }
}

impl AnyImpl for chrono::NaiveDateTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        HashMap::new()
    }
}

impl FromAnyRow for chrono::NaiveDateTime {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        match res {
            Ok(s) => {
                *index += 1;
                crate::temporal::parse_naive_datetime(&s).map_err(|e| Error::Decode(Box::new(e)))
            }
            Err(e) => {
                // Try numeric fallback (some drivers might return i64 for timestamps)
                if let Ok(i) = row.try_get::<i64, _>(*index) {
                    *index += 1;
                    return Ok(chrono::DateTime::from_timestamp(i, 0).map(|dt| dt.naive_utc()).unwrap_or_default());
                }
                // If both fail, we should still increment if it's likely a column was there but we couldn't decode it
                // Actually, for temporal it's tricky, but if it's NULL, both try_get will fail.
                // Let's check for NULL explicitly.
                if let Ok(None) = row.try_get::<Option<String>, _>(*index) {
                     *index += 1;
                     return Err(Error::Decode(Box::new(e))); // Option will catch this
                }

                Err(Error::Decode(Box::new(e)))
            }
        }
    }
}

impl AnyImpl for chrono::NaiveDate {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        HashMap::new()
    }
}

impl FromAnyRow for chrono::NaiveDate {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        *index += 1;
        let s = res.map_err(|e| Error::Decode(Box::new(e)))?;
        crate::temporal::parse_naive_date(&s).map_err(|e| Error::Decode(Box::new(e)))
    }
}

impl AnyImpl for chrono::NaiveTime {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        HashMap::new()
    }
}

impl FromAnyRow for chrono::NaiveTime {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        *index += 1;
        let s = res.map_err(|e| Error::Decode(Box::new(e)))?;
        crate::temporal::parse_naive_time(&s).map_err(|e| Error::Decode(Box::new(e)))
    }
}

impl AnyImpl for chrono::DateTime<chrono::Utc> {
    fn columns() -> Vec<AnyInfo> {
        Vec::new()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        HashMap::new()
    }
}

impl FromAnyRow for chrono::DateTime<chrono::Utc> {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        let mut index = 0;
        Self::from_any_row_at(row, &mut index)
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        if *index >= row.len() {
            return Err(Error::ColumnIndexOutOfBounds { index: *index, len: row.len() });
        }
        let res = row.try_get::<String, _>(*index);
        match res {
            Ok(s) => {
                *index += 1;
                crate::temporal::parse_datetime_utc(&s).map_err(|e| Error::Decode(Box::new(e)))
            }
            Err(e) => {
                // Try numeric fallback
                if let Ok(i) = row.try_get::<i64, _>(*index) {
                    *index += 1;
                    return Ok(chrono::DateTime::from_timestamp(i, 0).unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(chrono::NaiveDateTime::default(), chrono::Utc)));
                }
                
                if let Ok(None) = row.try_get::<Option<String>, _>(*index) {
                    *index += 1;
                    return Err(Error::Decode(Box::new(e)));
                }

                Err(Error::Decode(Box::new(e)))
            }
        }
    }
}

// ============================================================================
// Option Implementation
// ============================================================================

impl<T: AnyImpl> AnyImpl for Option<T> {
    fn columns() -> Vec<AnyInfo> {
        T::columns()
    }
    fn to_map(&self) -> HashMap<String, Option<String>> {
        match self {
            Some(v) => v.to_map(),
            None => HashMap::new(),
        }
    }
}

impl<T: FromAnyRow> FromAnyRow for Option<T> {
    fn from_any_row(row: &AnyRow) -> Result<Self, Error> {
        match T::from_any_row(row) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
        match T::from_any_row_at(row, index) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
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
                let mut cols = Vec::new();
                $(
                    cols.extend($T::columns());
                )+
                cols
            }

            fn to_map(&self) -> HashMap<String, Option<String>> {
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
                let mut index = 0;
                Ok((
                    $(
                        $T::from_any_row_at(row, &mut index)?,
                    )+
                ))
            }

            fn from_any_row_at(row: &AnyRow, index: &mut usize) -> Result<Self, Error> {
                Ok((
                    $(
                        $T::from_any_row_at(row, index)?,
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
