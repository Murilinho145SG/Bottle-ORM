//! # Error Handling Module
//!
//! This module defines the error types used throughout Bottle ORM.
//! It provides a centralized error handling system that wraps various error
//! scenarios that can occur during database operations.
//!
//! ## Error Types
//!
//! - **InvalidData**: Data validation errors (e.g., invalid format, constraint violations)
//! - **DatabaseError**: Wrapped sqlx errors (connection issues, query failures, etc.)
//! - **InvalidArgument**: Invalid arguments passed to ORM methods
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bottle_orm::Error;
//!
//! async fn create_user(db: &Database, age: i32) -> Result<User, Error> {
//!     if age < 0 {
//!         return Err(Error::InvalidData("Age cannot be negative".to_string()));
//!     }
//!
//!     let user = User { age, /* ... */ };
//!     db.model::<User>().insert(&user).await?;
//!     Ok(user)
//! }
//!
//! // Error handling
//! match create_user(&db, -5).await {
//!     Ok(user) => println!("Created: {:?}", user),
//!     Err(Error::InvalidData(msg)) => eprintln!("Validation error: {}", msg),
//!     Err(Error::DatabaseError(e)) => eprintln!("Database error: {}", e),
//!     Err(e) => eprintln!("Other error: {}", e),
//! }
//! ```

// ============================================================================
// External Crate Imports
// ============================================================================

use thiserror::Error;

// ============================================================================
// Error Enum Definition
// ============================================================================

/// The main error type for Bottle ORM operations.
///
/// This enum represents all possible errors that can occur during ORM operations.
/// It uses the `thiserror` crate to automatically implement `std::error::Error`
/// and provide helpful error messages.
///
/// # Variants
///
/// * `InvalidData` - Data validation errors
/// * `DatabaseError` - Wrapped sqlx database errors
/// * `InvalidArgument` - Invalid arguments passed to methods
///
/// # Display Format
///
/// Each variant has a custom display format defined via the `#[error(...)]` attribute:
///
/// - `InvalidData`: "Invalid Data {message}: {message}"
/// - `DatabaseError`: "Database error {inner_error}:"
/// - `InvalidArgument`: "Invalid argument {message}: {message}"
///
/// # Example
///
/// ```rust,ignore
/// use bottle_orm::Error;
///
/// fn validate_age(age: i32) -> Result<(), Error> {
///     if age < 0 {
///         return Err(Error::InvalidData("Age must be non-negative".to_string()));
///     }
///     if age > 150 {
///         return Err(Error::InvalidData("Age seems unrealistic".to_string()));
///     }
///     Ok(())
/// }
/// ```
#[derive(Error, Debug)]
pub enum Error {
    /// Invalid data error.
    ///
    /// This variant is used when data validation fails before or after
    /// a database operation. It typically indicates business logic violations
    /// rather than database-level constraints.
    ///
    /// # When to Use
    ///
    /// - Data format validation (e.g., email format, phone number)
    /// - Business rule violations (e.g., age limits, quantity constraints)
    /// - Type conversion failures
    /// - Serialization/deserialization errors
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn validate_email(email: &str) -> Result<(), Error> {
    ///     if !email.contains('@') {
    ///         return Err(Error::InvalidData(
    ///             format!("Invalid email format: {}", email)
    ///         ));
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[error("Invalid Data {0}: {0}")]
    InvalidData(String),

    /// Type conversion error.
    ///
    /// This variant is used when converting between Rust types and SQL types fails.
    /// It typically occurs during value binding or deserialization.
    ///
    /// # When to Use
    ///
    /// - Failed to parse string to DateTime, UUID, or numeric types
    /// - Type mismatch during value binding
    /// - Format conversion errors
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn parse_datetime(value: &str) -> Result<DateTime<Utc>, Error> {
    ///     value.parse::<DateTime<Utc>>()
    ///         .map_err(|e| Error::Conversion(format!("Failed to parse DateTime: {}", e)))
    /// }
    /// ```
    #[error("Type conversion error: {0}")]
    Conversion(String),

    /// Database operation error.
    ///
    /// This variant wraps errors from the underlying sqlx library.
    /// It's automatically converted from `sqlx::Error` via the `#[from]` attribute,
    /// making error propagation seamless with the `?` operator.
    ///
    /// # Common Causes
    ///
    /// - **Connection Errors**: Failed to connect to database, connection pool exhausted
    /// - **Query Errors**: SQL syntax errors, table/column not found
    /// - **Constraint Violations**: Primary key, foreign key, unique, not null violations
    /// - **Type Errors**: Type mismatch between Rust and SQL types
    /// - **Row Not Found**: `fetch_one()` or `first()` found no matching rows
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// async fn get_user_by_id(db: &Database, id: i32) -> Result<User, Error> {
    ///     // sqlx::Error is automatically converted to Error::DatabaseError
    ///     let user = db.model::<User>()
    ///         .filter("id", "=", id)
    ///         .first()
    ///         .await?;
    ///     Ok(user)
    /// }
    ///
    /// // Handling specific database errors
    /// match get_user_by_id(&db, 999).await {
    ///     Ok(user) => println!("Found: {:?}", user),
    ///     Err(Error::DatabaseError(e)) => {
    ///         if matches!(e, sqlx::Error::RowNotFound) {
    ///             eprintln!("User not found");
    ///         } else {
    ///             eprintln!("Database error: {}", e);
    ///         }
    ///     }
    ///     Err(e) => eprintln!("Other error: {}", e),
    /// }
    /// ```
    #[error("Database error {0}:")]
    DatabaseError(#[from] sqlx::Error),

    /// Invalid argument error.
    ///
    /// This variant is used when method arguments fail validation.
    /// It indicates programmer error (passing invalid parameters) rather than
    /// runtime data issues.
    ///
    /// # When to Use
    ///
    /// - Negative values where only positive are allowed
    /// - Out-of-range parameters (e.g., page number, limit)
    /// - Invalid enum values or flags
    /// - Null/empty values where required
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// impl QueryBuilder {
    ///     pub fn pagination(
    ///         mut self,
    ///         max_value: usize,
    ///         default: usize,
    ///         page: usize,
    ///         value: isize,
    ///     ) -> Result<Self, Error> {
    ///         // Validate argument
    ///         if value < 0 {
    ///             return Err(Error::InvalidArgument(
    ///                 "value cannot be negative".to_string()
    ///             ));
    ///         }
    ///
    ///         // ... rest of implementation
    ///         Ok(self)
    ///     }
    /// }
    /// ```
    #[error("Invalid argument {0}: {0}")]
    InvalidArgument(String),
}

// ============================================================================
// Error Conversion Implementations
// ============================================================================

/// Automatic conversion from `sqlx::Error` to `Error::DatabaseError`.
///
/// This is provided automatically by the `#[from]` attribute on the
/// `DatabaseError` variant. It enables using the `?` operator to propagate
/// sqlx errors as Bottle ORM errors.
///
/// # Example
///
/// ```rust,ignore
/// async fn example(db: &Database) -> Result<Vec<User>, Error> {
///     // sqlx::Error is automatically converted to Error via ?
///     let users = db.model::<User>().scan().await?;
///     Ok(users)
/// }
/// ```

// ============================================================================
// Helper Functions and Traits
// ============================================================================

impl Error {
    /// Creates an `InvalidData` error from a string slice.
    ///
    /// This is a convenience method to avoid calling `.to_string()` manually.
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn validate(value: i32) -> Result<(), Error> {
    ///     if value < 0 {
    ///         return Err(Error::invalid_data("Value must be positive"));
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn invalid_data(msg: &str) -> Self {
        Error::InvalidData(msg.to_string())
    }

    /// Creates an `InvalidArgument` error from a string slice.
    ///
    /// This is a convenience method to avoid calling `.to_string()` manually.
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn set_limit(limit: isize) -> Result<(), Error> {
    ///     if limit < 0 {
    ///         return Err(Error::invalid_argument("Limit cannot be negative"));
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn invalid_argument(msg: &str) -> Self {
        Error::InvalidArgument(msg.to_string())
    }

    /// Creates a `Conversion` error from a string slice.
    ///
    /// This is a convenience method to avoid calling `.to_string()` manually.
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn parse_value(value: &str) -> Result<i32, Error> {
    ///     value.parse::<i32>()
    ///         .map_err(|_| Error::conversion("Invalid integer format"))
    /// }
    /// ```
    pub fn conversion(msg: &str) -> Self {
        Error::Conversion(msg.to_string())
    }
}
