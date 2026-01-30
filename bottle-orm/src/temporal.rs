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

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::any::AnyArguments;
use sqlx::Arguments;

use crate::database::Drivers;
use crate::Error;

// ============================================================================
// DateTime<Utc> and DateTime<FixedOffset> Conversion
// ============================================================================

/// Binds a `DateTime<Utc>` value to a SQL query based on the database driver.
pub fn bind_datetime_utc(
    query_args: &mut AnyArguments<'_>,
    value: &DateTime<Utc>,
    driver: &Drivers,
) -> Result<(), Error> {
    match driver {
        Drivers::Postgres => {
            let _ = query_args.add(value.to_rfc3339());
        }
        Drivers::MySQL => {
            let formatted = value.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
        Drivers::SQLite => {
            let _ = query_args.add(value.to_rfc3339());
        }
    }
    Ok(())
}

/// Binds a `DateTime<FixedOffset>` value.
pub fn bind_datetime_fixed(
    query_args: &mut AnyArguments<'_>,
    value: &DateTime<FixedOffset>,
    driver: &Drivers,
) -> Result<(), Error> {
    // Convert to UTC for consistency across drivers that enforce UTC
    // or keep offset depending on driver capabilities.
    // For simplicity and consistency with existing logic, we bind as string.
    match driver {
        Drivers::Postgres => {
            // Postgres handles offsets fine in TIMESTAMPTZ
            let _ = query_args.add(value.to_rfc3339());
        }
        Drivers::MySQL => {
            // MySQL converts to UTC for TIMESTAMP storage anyway
            let value_utc: DateTime<Utc> = value.with_timezone(&Utc);
            let formatted = value_utc.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
            let _ = query_args.add(formatted);
        }
        Drivers::SQLite => {
            // SQLite uses text, so RFC3339 with offset is fine
            let _ = query_args.add(value.to_rfc3339());
        }
    }
    Ok(())
}

/// Parses a string into a `DateTime<Utc>`.
///
/// Tries strict `DateTime<Utc>` parsing first. If that fails, tries parsing as
/// `DateTime<FixedOffset>` and converting to UTC. This supports inputs with
/// arbitrary timezones (e.g. "+02:00").
pub fn parse_datetime_utc(value: &str) -> Result<DateTime<Utc>, Error> {
    // Try direct UTC parsing
    if let Ok(dt) = value.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }

    // Try FixedOffset parsing and convert to UTC
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try parsing without timezone (Naive) and assume UTC
    // This handles "YYYY-MM-DD HH:MM:SS" formats common in MySQL/SQLite
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    Err(Error::Conversion(format!("Failed to parse DateTime<Utc> from '{}'", value)))
}

/// Parses a string into a `DateTime<FixedOffset>`.
pub fn parse_datetime_fixed(value: &str) -> Result<DateTime<FixedOffset>, Error> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt);
    }

    // If it lacks timezone info (Naive), we generally assume UTC for safety
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        // Create a FixedOffset of +00:00 (UTC)
        let offset = FixedOffset::east_opt(0).unwrap();
        return Ok(DateTime::from_naive_utc_and_offset(naive, offset));
    }

    Err(Error::Conversion(format!("Failed to parse DateTime<FixedOffset> from '{}'", value)))
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
    // Try default parsing
    if let Ok(dt) = value.parse::<NaiveDateTime>() {
        return Ok(dt);
    }

    // Try formats common in different DBs
    // YYYY-MM-DD HH:MM:SS
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt);
    }

    // YYYY-MM-DD HH:MM:SS.f
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(dt);
    }

    // YYYY-MM-DD HH:MM (no seconds)
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M") {
        return Ok(dt);
    }

    // RFC 3339 (T separator)
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(dt);
    }

    Err(Error::Conversion(format!("Failed to parse NaiveDateTime from '{}'", value)))
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

/// Converts a `DateTime<FixedOffset>` to the format expected by a specific driver.
pub fn format_datetime_fixed_for_driver(value: &DateTime<FixedOffset>, driver: &Drivers) -> String {
    match driver {
        Drivers::Postgres => value.to_rfc3339(),
        Drivers::MySQL => {
            // Convert to UTC for MySQL
            let value_utc: DateTime<Utc> = value.with_timezone(&Utc);
            value_utc.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
        }
        Drivers::SQLite => value.to_rfc3339(),
    }
}

/// Converts a `NaiveDateTime` to the format expected by a specific driver.
pub fn format_naive_datetime_for_driver(value: &NaiveDateTime, driver: &Drivers) -> String {
    match driver {
        Drivers::Postgres | Drivers::MySQL => value.format("%Y-%m-%d %H:%M:%S%.6f").to_string(),
        Drivers::SQLite => value.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
    }
}
