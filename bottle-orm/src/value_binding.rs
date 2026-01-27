//! # Value Binding Module
//!
//! This module provides type-safe value binding utilities for SQL queries.
//! It handles conversion from Rust types to database-native types across
//! different database drivers (PostgreSQL, MySQL, SQLite).
//!
//! ## Features
//!
//! - **Type-Safe Binding**: Automatic type detection and conversion
//! - **Driver-Specific Optimization**: Uses native types when possible
//! - **Temporal Type Support**: Specialized handling for DateTime types via temporal module
//! - **UUID Support**: Handles all UUID versions (1-7)
//! - **Error Handling**: Graceful fallback for parsing errors

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::any::AnyArguments;
use sqlx::Arguments;
use uuid::Uuid;

use crate::{database::Drivers, temporal, Error};

// ============================================================================
// Value Binding Trait
// ============================================================================

/// Extension trait for binding values to AnyArguments with driver-specific handling.
pub trait ValueBinder {
    /// Binds a value to the arguments based on its SQL type and database driver.
    ///
    /// # Arguments
    ///
    /// * `value_str` - String representation of the value
    /// * `sql_type` - SQL type identifier (e.g., "INTEGER", "TEXT", "TIMESTAMPTZ")
    /// * `driver` - Database driver being used
    ///
    /// # Returns
    ///
    /// `Ok(())` if binding succeeds, `Err(Error)` otherwise
    fn bind_value(&mut self, value_str: &str, sql_type: &str, driver: &Drivers) -> Result<(), Error>;

    /// Binds an integer value (i32).
    fn bind_i32(&mut self, value: i32);

    /// Binds a big integer value (i64).
    fn bind_i64(&mut self, value: i64);

    /// Binds a boolean value.
    fn bind_bool(&mut self, value: bool);

    /// Binds a floating-point value (f64).
    fn bind_f64(&mut self, value: f64);

    /// Binds a string value.
    fn bind_string(&mut self, value: String);

    /// Binds a UUID value.
    fn bind_uuid(&mut self, value: Uuid, driver: &Drivers);

    /// Binds a DateTime<Utc> value.
    fn bind_datetime_utc(&mut self, value: DateTime<Utc>, driver: &Drivers);

    /// Binds a DateTime<FixedOffset> value.
    fn bind_datetime_fixed(&mut self, value: chrono::DateTime<chrono::FixedOffset>, driver: &Drivers);

    /// Binds a NaiveDateTime value.
    fn bind_naive_datetime(&mut self, value: NaiveDateTime, driver: &Drivers);

    /// Binds a NaiveDate value.
    fn bind_naive_date(&mut self, value: NaiveDate, driver: &Drivers);

    /// Binds a NaiveTime value.
    fn bind_naive_time(&mut self, value: NaiveTime, driver: &Drivers);
}

impl ValueBinder for AnyArguments<'_> {
    fn bind_value(&mut self, value_str: &str, sql_type: &str, driver: &Drivers) -> Result<(), Error> {
        match sql_type {
            // ================================================================
            // Integer Types
            // ================================================================
            "INTEGER" | "INT" | "SERIAL" | "serial" | "int4" => {
                // Try parsing as i32 first, fallback to u32/i64 if needed but sql_type says INTEGER
                if let Ok(val) = value_str.parse::<i32>() {
                    self.bind_i32(val);
                } else if let Ok(val) = value_str.parse::<u32>() {
                    self.bind_i64(val as i64); // Map u32 to i64 to fit
                } else {
                    return Err(Error::Conversion(format!("Failed to parse integer: {}", value_str)));
                }
                Ok(())
            }

            "BIGINT" | "INT8" | "int8" | "BIGSERIAL" => {
                if let Ok(val) = value_str.parse::<i64>() {
                    self.bind_i64(val);
                } else if let Ok(_val) = value_str.parse::<u64>() {
                    // u64 might overflow i64, strictly speaking, but standard mapping in rust sqlx usually handles i64
                    // We'll try to bind as i64 (unsafe cast) or string?
                    // Best effort: bind as i64 (reinterpreting bits or clamping? No, let's just parse)
                    // If it exceeds i64::MAX, it's an issue for standard SQL BIGINT (signed).
                    // For now, parse as i64.
                    let val = value_str
                        .parse::<i64>()
                        .map_err(|e| Error::Conversion(format!("Failed to parse i64: {}", e)))?;
                    self.bind_i64(val);
                } else {
                    return Err(Error::Conversion(format!("Failed to parse i64: {}", value_str)));
                }
                Ok(())
            }

            "SMALLINT" | "INT2" | "int2" => {
                let val: i16 =
                    value_str.parse().map_err(|e| Error::Conversion(format!("Failed to parse i16: {}", e)))?;
                let _ = self.add(val);
                Ok(())
            }

            // ================================================================
            // Boolean Type
            // ================================================================
            "BOOLEAN" | "BOOL" | "bool" => {
                let val: bool =
                    value_str.parse().map_err(|e| Error::Conversion(format!("Failed to parse bool: {}", e)))?;
                self.bind_bool(val);
                Ok(())
            }

            // ================================================================
            // Floating-Point Types
            // ================================================================
            "DOUBLE PRECISION" | "FLOAT" | "float8" | "NUMERIC" | "DECIMAL" => {
                let val: f64 =
                    value_str.parse().map_err(|e| Error::Conversion(format!("Failed to parse f64: {}", e)))?;
                self.bind_f64(val);
                Ok(())
            }

            "REAL" | "float4" => {
                let val: f32 =
                    value_str.parse().map_err(|e| Error::Conversion(format!("Failed to parse f32: {}", e)))?;
                let _ = self.add(val);
                Ok(())
            }

            // ================================================================
            // JSON Types
            // ================================================================
            "JSON" | "JSONB" | "json" | "jsonb" => {
                // Determine driver-specific JSON handling
                match driver {
                    Drivers::Postgres => {
                        // For Postgres, we can bind as serde_json::Value if sqlx supports it,
                        // or bind as string/text but rely on Postgres casting `::JSONB` in the query string.
                        // The QueryBuilder handles the `::JSONB` cast in the SQL string.
                        // So we just bind the string representation here.
                        self.bind_string(value_str.to_string());
                    }
                    _ => {
                        self.bind_string(value_str.to_string());
                    }
                }
                Ok(())
            }

            // ================================================================
            // UUID Type
            // ================================================================
            "UUID" => {
                let val =
                    value_str.parse::<Uuid>().map_err(|e| Error::Conversion(format!("Failed to parse UUID: {}", e)))?;
                self.bind_uuid(val, driver);
                Ok(())
            }

            // ================================================================
            // Temporal Types (DateTime, Date, Time)
            // ================================================================
            "TIMESTAMPTZ" | "DateTime" => {
                // Try parsing as UTC first
                if let Ok(val) = temporal::parse_datetime_utc(value_str) {
                    self.bind_datetime_utc(val, driver);
                } else if let Ok(val) = temporal::parse_datetime_fixed(value_str) {
                    // Fallback to FixedOffset if UTC fails (though parse_datetime_utc handles fixed too)
                    self.bind_datetime_fixed(val, driver);
                } else {
                    return Err(Error::Conversion(format!("Failed to parse DateTime: {}", value_str)));
                }
                Ok(())
            }

            "TIMESTAMP" | "NaiveDateTime" => {
                let val = temporal::parse_naive_datetime(value_str)?;
                self.bind_naive_datetime(val, driver);
                Ok(())
            }

            "DATE" | "NaiveDate" => {
                let val = temporal::parse_naive_date(value_str)?;
                self.bind_naive_date(val, driver);
                Ok(())
            }

            "TIME" | "NaiveTime" => {
                let val = temporal::parse_naive_time(value_str)?;
                self.bind_naive_time(val, driver);
                Ok(())
            }

            // ================================================================
            // Text and Default Types
            // ================================================================
            "TEXT" | "VARCHAR" | "CHAR" | "STRING" | _ => {
                self.bind_string(value_str.to_string());
                Ok(())
            }
        }
    }

    fn bind_i32(&mut self, value: i32) {
        let _ = self.add(value);
    }

    fn bind_i64(&mut self, value: i64) {
        let _ = self.add(value);
    }

    fn bind_bool(&mut self, value: bool) {
        let _ = self.add(value);
    }

    fn bind_f64(&mut self, value: f64) {
        let _ = self.add(value);
    }

    fn bind_string(&mut self, value: String) {
        let _ = self.add(value);
    }

    fn bind_uuid(&mut self, value: Uuid, driver: &Drivers) {
        match driver {
            Drivers::Postgres => {
                // PostgreSQL has native UUID support
                // Convert to hyphenated string format
                let _ = self.add(value.hyphenated().to_string());
            }
            Drivers::MySQL => {
                // MySQL stores UUID as CHAR(36)
                let _ = self.add(value.hyphenated().to_string());
            }
            Drivers::SQLite => {
                // SQLite stores as TEXT
                let _ = self.add(value.hyphenated().to_string());
            }
        }
    }

    fn bind_datetime_utc(&mut self, value: DateTime<Utc>, driver: &Drivers) {
        let formatted = temporal::format_datetime_for_driver(&value, driver);
        let _ = self.add(formatted);
    }

    fn bind_datetime_fixed(&mut self, value: chrono::DateTime<chrono::FixedOffset>, driver: &Drivers) {
        let formatted = temporal::format_datetime_fixed_for_driver(&value, driver);
        let _ = self.add(formatted);
    }

    fn bind_naive_datetime(&mut self, value: NaiveDateTime, driver: &Drivers) {
        let formatted = temporal::format_naive_datetime_for_driver(&value, driver);
        let _ = self.add(formatted);
    }

    fn bind_naive_date(&mut self, value: NaiveDate, _driver: &Drivers) {
        // All drivers use ISO 8601 date format
        let formatted = value.format("%Y-%m-%d").to_string();
        let _ = self.add(formatted);
    }

    fn bind_naive_time(&mut self, value: NaiveTime, _driver: &Drivers) {
        // All drivers use ISO 8601 time format
        let formatted = value.format("%H:%M:%S%.6f").to_string();
        let _ = self.add(formatted);
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Binds a value to AnyArguments with automatic type detection and conversion.
///
/// This is a convenience function that wraps the ValueBinder trait.
///
/// # Arguments
///
/// * `args` - The AnyArguments to bind the value to
/// * `value_str` - String representation of the value
/// * `sql_type` - SQL type identifier
/// * `driver` - Database driver
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::value_binding::bind_typed_value;
/// use sqlx::any::AnyArguments;
///
/// let mut args = AnyArguments::default();
/// bind_typed_value(&mut args, "42", "INTEGER", &Drivers::Postgres)?;
/// bind_typed_value(&mut args, "2024-01-15T14:30:00+00:00", "TIMESTAMPTZ", &Drivers::Postgres)?;
/// ```
pub fn bind_typed_value(
    args: &mut AnyArguments<'_>,
    value_str: &str,
    sql_type: &str,
    driver: &Drivers,
) -> Result<(), Error> {
    args.bind_value(value_str, sql_type, driver)
}

/// Attempts to bind a value, falling back to string binding on error.
///
/// This is useful for cases where you want to be more lenient with type conversion.
///
/// # Arguments
///
/// * `args` - The AnyArguments to bind the value to
/// * `value_str` - String representation of the value
/// * `sql_type` - SQL type identifier
/// * `driver` - Database driver
pub fn bind_typed_value_or_string(args: &mut AnyArguments<'_>, value_str: &str, sql_type: &str, driver: &Drivers) {
    if let Err(_) = args.bind_value(value_str, sql_type, driver) {
        // Fallback: bind as string
        let _ = args.add(value_str.to_string());
    }
}

// ============================================================================
// Type Detection
// ============================================================================

/// Detects if a SQL type requires special handling.
pub fn requires_special_binding(sql_type: &str) -> bool {
    matches!(
        sql_type,
        "UUID"
            | "TIMESTAMPTZ"
            | "DateTime"
            | "TIMESTAMP"
            | "NaiveDateTime"
            | "DATE"
            | "NaiveDate"
            | "TIME"
            | "NaiveTime"
    )
}

/// Returns whether a SQL type is numeric.
pub fn is_numeric_type(sql_type: &str) -> bool {
    matches!(
        sql_type,
        "INTEGER"
            | "INT"
            | "BIGINT"
            | "INT8"
            | "SERIAL"
            | "BIGSERIAL"
            | "SMALLINT"
            | "DOUBLE PRECISION"
            | "FLOAT"
            | "REAL"
            | "NUMERIC"
            | "DECIMAL"
    )
}

/// Returns whether a SQL type is textual.
pub fn is_text_type(sql_type: &str) -> bool {
    matches!(sql_type, "TEXT" | "VARCHAR" | "CHAR" | "STRING")
}
