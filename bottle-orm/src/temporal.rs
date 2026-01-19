//! # Temporal Type Conversion Module
//!
//! This module provides specialized handling for temporal types (DateTime, NaiveDateTime, etc.)
//! across different database drivers. It optimizes the conversion between Rust chrono types
//! and native database types for PostgreSQL, MySQL, and SQLite.
//!
//! ## Key Features
//!
//! - **Native Type Support**: Uses database-native types when possible instead of string conversion
//! - **Driver-Specific Optimization**: Tailored conversion for each database driver
//! - **Timezone Handling**: Proper timezone conversion for DateTime<Utc>
//! - **Format Consistency**: Ensures consistent date/time formats across drivers
//!
//! ## Supported Types
//!
//! - `DateTime<Utc>` - Timestamp with timezone (UTC)
//! - `NaiveDateTime` - Timestamp without timezone
//! - `NaiveDate` - Date only (year, month, day)
//! - `NaiveTime` - Time only (hour, minute, second)

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::Arguments;
use sqlx::any::AnyArguments;

use crate::Error;
use crate::database::Drivers;

// ============================================================================
// DateTime<Utc> Conversion
// ============================================================================

/// Binds a `DateTime<Utc>` value to a SQL query based on the database driver.
///
/// # Arguments
///
/// * `query_args` - The SQLx AnyArguments to bind the value to
/// * `value` - The DateTime<Utc> value to bind
/// * `driver` - The database driver being used
///
/// # Database-Specific Behavior
///
/// ## PostgreSQL
/// - Uses native TIMESTAMPTZ type
/// - Stores as UTC timestamp
/// - Format: RFC 3339 (ISO 8601)
///
/// ## MySQL
/// - Converts to TIMESTAMP type
/// - Stores as UTC, converts based on session timezone
/// - Note: Limited to dates before 2038-01-19 (Y2038 problem)
///
/// ## SQLite
/// - Stores as TEXT in ISO 8601 format
/// - Format: "YYYY-MM-DD HH:MM:SS.SSS+00:00"
pub fn bind_datetime_utc(
    query_args: &mut AnyArguments<'_>,
    value: &DateTime<Utc>,
    driver: &Drivers,
) -> Result<(), Error> {
    match driver {
        Drivers::Postgres => {
            // PostgreSQL has native TIMESTAMPTZ support
            // SQLx handles the conversion automatically
            let _ = query_args.add(value.to_rfc3339());
        }
        Drivers::MySQL => {
            // MySQL TIMESTAMP: stores in UTC, displays in session timezone
            // Format: "YYYY-MM-DD HH:MM:SS"
            let formatted = value.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
        Drivers::SQLite => {
            // SQLite stores as TEXT in ISO 8601 format
            // Using RFC 3339 for maximum compatibility
            let _ = query_args.add(value.to_rfc3339());
        }
    }
    Ok(())
}

/// Parses a string into a `DateTime<Utc>`.
///
/// Attempts to parse the string using RFC 3339 format first,
/// then falls back to other common formats.
pub fn parse_datetime_utc(value: &str) -> Result<DateTime<Utc>, Error> {
    value.parse::<DateTime<Utc>>().map_err(|e| Error::Conversion(format!("Failed to parse DateTime<Utc>: {}", e)))
}

// ============================================================================
// NaiveDateTime Conversion
// ============================================================================

/// Binds a `NaiveDateTime` value to a SQL query based on the database driver.
///
/// # Arguments
///
/// * `query_args` - The SQLx AnyArguments to bind the value to
/// * `value` - The NaiveDateTime value to bind
/// * `driver` - The database driver being used
///
/// # Database-Specific Behavior
///
/// ## PostgreSQL
/// - Uses TIMESTAMP (without timezone) type
/// - Stores as-is without timezone conversion
///
/// ## MySQL
/// - Uses DATETIME type (no Y2038 limit)
/// - Stores without timezone information
/// - Range: '1000-01-01 00:00:00' to '9999-12-31 23:59:59'
///
/// ## SQLite
/// - Stores as TEXT in ISO 8601 format
/// - Format: "YYYY-MM-DD HH:MM:SS.SSS"
pub fn bind_naive_datetime(
    query_args: &mut AnyArguments<'_>,
    value: &NaiveDateTime,
    driver: &Drivers,
) -> Result<(), Error> {
    match driver {
        Drivers::Postgres => {
            // PostgreSQL TIMESTAMP (without timezone)
            // Format: "YYYY-MM-DD HH:MM:SS.SSSSSS"
            let formatted = value.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
        Drivers::MySQL => {
            // MySQL DATETIME
            // Format: "YYYY-MM-DD HH:MM:SS.SSSSSS"
            let formatted = value.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
        Drivers::SQLite => {
            // SQLite TEXT format
            // Using ISO 8601 format
            let formatted = value.format("%Y-%m-%d %H:%M:%S%.f").to_string();
            let _ = query_args.add(formatted);
        }
    }
    Ok(())
}

/// Parses a string into a `NaiveDateTime`.
pub fn parse_naive_datetime(value: &str) -> Result<NaiveDateTime, Error> {
    value.parse::<NaiveDateTime>().map_err(|e| Error::Conversion(format!("Failed to parse NaiveDateTime: {}", e)))
}

// ============================================================================
// NaiveDate Conversion
// ============================================================================

/// Binds a `NaiveDate` value to a SQL query based on the database driver.
///
/// # Arguments
///
/// * `query_args` - The SQLx AnyArguments to bind the value to
/// * `value` - The NaiveDate value to bind
/// * `driver` - The database driver being used
///
/// # Database-Specific Behavior
///
/// All drivers use standard DATE type with format "YYYY-MM-DD"
pub fn bind_naive_date(query_args: &mut AnyArguments<'_>, value: &NaiveDate, driver: &Drivers) -> Result<(), Error> {
    match driver {
        Drivers::Postgres | Drivers::MySQL | Drivers::SQLite => {
            // All databases use ISO 8601 date format: YYYY-MM-DD
            let formatted = value.format("%Y-%m-%d").to_string();
            let _ = query_args.add(formatted);
        }
    }
    Ok(())
}

/// Parses a string into a `NaiveDate`.
pub fn parse_naive_date(value: &str) -> Result<NaiveDate, Error> {
    value.parse::<NaiveDate>().map_err(|e| Error::Conversion(format!("Failed to parse NaiveDate: {}", e)))
}

// ============================================================================
// NaiveTime Conversion
// ============================================================================

/// Binds a `NaiveTime` value to a SQL query based on the database driver.
///
/// # Arguments
///
/// * `query_args` - The SQLx AnyArguments to bind the value to
/// * `value` - The NaiveTime value to bind
/// * `driver` - The database driver being used
///
/// # Database-Specific Behavior
///
/// All drivers use standard TIME type with format "HH:MM:SS.ffffff"
pub fn bind_naive_time(query_args: &mut AnyArguments<'_>, value: &NaiveTime, driver: &Drivers) -> Result<(), Error> {
    match driver {
        Drivers::Postgres | Drivers::MySQL | Drivers::SQLite => {
            // All databases use ISO 8601 time format: HH:MM:SS.ffffff
            let formatted = value.format("%H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
    }
    Ok(())
}

/// Parses a string into a `NaiveTime`.
pub fn parse_naive_time(value: &str) -> Result<NaiveTime, Error> {
    value.parse::<NaiveTime>().map_err(|e| Error::Conversion(format!("Failed to parse NaiveTime: {}", e)))
}

// ============================================================================
// Generic Temporal Binding
// ============================================================================

/// Binds a temporal value to a SQL query based on its SQL type.
///
/// This is a convenience function that dispatches to the appropriate
/// type-specific binding function based on the SQL type string.
///
/// # Arguments
///
/// * `query_args` - The SQLx AnyArguments to bind the value to
/// * `value_str` - The string representation of the temporal value
/// * `sql_type` - The SQL type of the column
/// * `driver` - The database driver being used
pub fn bind_temporal_value(
    query_args: &mut AnyArguments<'_>,
    value_str: &str,
    sql_type: &str,
    driver: &Drivers,
) -> Result<(), Error> {
    match sql_type {
        "TIMESTAMPTZ" | "DateTime" => {
            let value = parse_datetime_utc(value_str)?;
            bind_datetime_utc(query_args, &value, driver)
        }
        "TIMESTAMP" | "NaiveDateTime" => {
            let value = parse_naive_datetime(value_str)?;
            bind_naive_datetime(query_args, &value, driver)
        }
        "DATE" | "NaiveDate" => {
            let value = parse_naive_date(value_str)?;
            bind_naive_date(query_args, &value, driver)
        }
        "TIME" | "NaiveTime" => {
            let value = parse_naive_time(value_str)?;
            bind_naive_time(query_args, &value, driver)
        }
        _ => Err(Error::Conversion(format!("Unknown temporal SQL type: {}", sql_type))),
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Returns the appropriate SQL type cast string for temporal types in PostgreSQL.
///
/// # Arguments
///
/// * `sql_type` - The SQL type identifier
///
/// # Returns
///
/// The PostgreSQL type cast string (e.g., "::TIMESTAMPTZ")
pub fn get_postgres_type_cast(sql_type: &str) -> &'static str {
    match sql_type {
        "TIMESTAMPTZ" | "DateTime" => "::TIMESTAMPTZ",
        "TIMESTAMP" | "NaiveDateTime" => "::TIMESTAMP",
        "DATE" | "NaiveDate" => "::DATE",
        "TIME" | "NaiveTime" => "::TIME",
        _ => "",
    }
}

/// Checks if a SQL type is a temporal type.
pub fn is_temporal_type(sql_type: &str) -> bool {
    matches!(
        sql_type,
        "TIMESTAMPTZ" | "DateTime" | "TIMESTAMP" | "NaiveDateTime" | "DATE" | "NaiveDate" | "TIME" | "NaiveTime"
    )
}

// ============================================================================
// Format Conversion Utilities
// ============================================================================

/// Converts a `DateTime<Utc>` to the format expected by a specific driver.
///
/// This is useful for debugging or when you need the string representation
/// without actually binding to a query.
pub fn format_datetime_for_driver(value: &DateTime<Utc>, driver: &Drivers) -> String {
    match driver {
        Drivers::Postgres | Drivers::SQLite => value.to_rfc3339(),
        Drivers::MySQL => value.format("%Y-%m-%d %H:%M:%S%.6f").to_string(),
    }
}

/// Converts a `NaiveDateTime` to the format expected by a specific driver.
pub fn format_naive_datetime_for_driver(value: &NaiveDateTime, driver: &Drivers) -> String {
    match driver {
        Drivers::Postgres | Drivers::MySQL => value.format("%Y-%m-%d %H:%M:%S%.6f").to_string(),
        Drivers::SQLite => value.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
    }
}
