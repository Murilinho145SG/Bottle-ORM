# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-01-21

### Added
- **JOIN Support**: Implemented `join()` method in `QueryBuilder` to allow table joins.
  - Added support for qualified column names (e.g., `table.column`) in select and filter clauses to prevent ambiguity.
- **UUID Support**: Added direct support for parsing `Uuid` types in `FromAnyRow` derive macro.

### Changed
- **Foreign Key Validation**: Now enforces `Option<T>` type for fields annotated with `#[foreign_key]` to ensure correct nullability handling.

### Fixed
- **Query Builder**: Resolved column ambiguity issues in SQL generation when using joins.
- **Cleanup**: Removed debug print statements from `scalar` query execution.

## [0.2.2-rc.3] - 2026-01-20

### Added
- **Update & Delete Support**: Implemented comprehensive update and delete capabilities in `QueryBuilder`.
  - `update(col, value)`: Efficiently update a single column with type safety.
  - `updates(model)`: Update all active columns using a full model instance.
  - `update_partial(partial)`: Update a specific subset of columns using a custom partial struct (via `AnyImpl`).
  - `delete()`: Delete rows matching the current filter criteria.
- **AnyImpl Enhancements**: Added `to_map()` to `AnyImpl` trait, enabling partial structs to be used for dynamic update queries.
- **JOIN Support Preparation**: Added `joins_clauses` field to `QueryBuilder` structure to support future JOIN operations.

### Fixed
- **Query Builder Ordering**: Fixed `ORDER BY` clauses not being applied in `scan()` and `first()` methods.
- **SQL Generation**: Fixed invalid SQL generation when multiple `order()` calls are chained (now correctly comma-separated).
- **Deterministic Ordering**: Improved `first()` method to strictly respect user ordering if provided, falling back to Primary Key ordering only when no specific order is requested.


### Added

#### AnyImpl & FromAnyRow Support
- **Macro `FromAnyRow`**: New derive macro for scanning arbitrary query results into structs
  - Allows mapping `sqlx::any::AnyRow` to custom structs
  - Handles type conversions automatically, with special logic for `DateTime`
  - Eliminates the need for manual `FromRow` implementation for complex queries

- **Trait `AnyImpl` & Struct `AnyInfo`**: New metadata system for dynamic row mapping
  - `AnyImpl`: Trait for types that can be scanned from `AnyRow`
  - `AnyInfo`: Struct containing column metadata (name, SQL type)
  - Helper macro `impl_any_primitive!` for basic types
  - Implementations for standard types (`bool`, integers, floats, `String`, `Uuid`, `chrono` types)

- **QueryBuilder Integration**: Updated `scan()` and `first()` to support `AnyImpl`
  - Seamless integration with `FromAnyRow` derived structs
  - Automatic `to_json` casting for temporal types (`DateTime`, `NaiveDateTime`, etc.) in SELECT clauses to ensure compatibility across drivers when using `AnyRow`

#### Query Builder Enhancements
- **Method `scalar()`**: Added support for fetching single scalar values directly
  - Enables intuitive queries like `let count: i64 = query.select("count(*)").scalar().await?;`
  - Bypasses `FromRow` requirement for simple primitive types (`i32`, `String`, etc.)

- **Tuple Support**: Implemented `AnyImpl` for tuples (up to 8 elements)
  - Allows scanning results directly into tuples: `let (id, name): (i32, String) = ...`

#### DateTime Temporal Type Conversion System
- **Module `temporal.rs`**: Specialized system for temporal type conversions
  - Parsing functions with error handling: `parse_datetime_utc()`, `parse_naive_datetime()`, `parse_naive_date()`, `parse_naive_time()`
  - Driver-optimized formatting: `format_datetime_for_driver()`, `format_naive_datetime_for_driver()`
  - Temporal value binding: `bind_datetime_utc()`, `bind_naive_datetime()`, `bind_naive_date()`, `bind_naive_time()`
  - Utilities: `is_temporal_type()`, `get_postgres_type_cast()`
  - Full support for `DateTime<Utc>`, `NaiveDateTime`, `NaiveDate`, `NaiveTime`
  - PostgreSQL: RFC 3339 format for `DateTime<Utc>`, microsecond precision for `NaiveDateTime`
  - MySQL: Optimized format for TIMESTAMP/DATETIME types, handles Y2038 limitation awareness
  - SQLite: ISO 8601 format compatible with SQLite date/time functions

- **Module `value_binding.rs`**: Type-safe value binding system for SQL queries
  - `ValueBinder` trait for automatic type detection and binding
  - Support for primitive types: i32, i64, bool, f64, String
  - Support for UUID (all versions 1-7)
  - Support for temporal types via `temporal` module integration
  - Helper functions: `bind_typed_value()`, `bind_typed_value_or_string()`
  - Type detection: `requires_special_binding()`, `is_numeric_type()`, `is_text_type()`

- **Error Variant `Conversion`**: New variant in `Error` enum
  - Specific handling for type conversion errors
  - Descriptive error messages with context
  - Helper function: `Error::conversion()`

- **Example `examples/datetime_conversion.rs`**: Runnable example demonstrating:
  - Driver-specific formatting
  - Type detection utilities
  - PostgreSQL type casting
  - Parsing examples with error handling
  - Best practices for each database

- **Example `examples/basic_usage.rs`**: Basic usage example in Portuguese
  - Simple CRUD operations with DateTime
  - Model definition with temporal types
  - Database connection and migrations
  - Formatting examples

#### UUID Support (Versions 1-7)
- **Full UUID Support**: Added comprehensive support for all UUID versions (1 through 7)
  - Version 1: Time-based with MAC address
  - Version 3: Name-based using MD5 hash
  - Version 4: Random (most common)
  - Version 5: Name-based using SHA-1 hash
  - Version 6: Reordered time-based (better for database indexing)
  - Version 7: Unix timestamp-based (sortable, recommended for new projects)
- Added `uuid` dependency with features for all versions: `v1`, `v3`, `v4`, `v5`, `v6`, `v7`, `serde`
- Updated type mapping in `types.rs` to handle `Uuid` → `UUID` SQL type
- Updated `query_builder.rs` to properly bind UUID values in INSERT operations
- Added UUID examples in README.md demonstrating usage with different versions

#### Documentation Improvements
- **Comprehensive Code Comments**: Added detailed documentation following Rust best practices
  - Module-level documentation for all files
  - Function-level documentation with examples and parameter descriptions
  - Inline comments explaining complex logic
  - Type and trait documentation with usage examples
  
- **Organized Structure**: Improved code organization
  - Clear section separators with comment blocks
  - Grouped related functionality
  - Consistent comment style across all modules

### Changed

#### DateTime Conversion Improvements
- **query_builder.rs**: Refactored to use `temporal` and `value_binding` modules
  - Replaced naive `to_string()` conversions with driver-optimized formatting
  - Added proper error handling for temporal type conversions
  - Implemented PostgreSQL explicit type casting (e.g., `$1::TIMESTAMPTZ`)
  - Reduced code duplication by using centralized binding functions
  - Improved maintainability and testability

- **Type Mapping**: Enhanced temporal type handling in `types.rs`
  - `DateTime<Utc>` → `TIMESTAMPTZ` (PostgreSQL native timezone support)
  - `NaiveDateTime` → `TIMESTAMP` (PostgreSQL) / `DATETIME` (MySQL, no Y2038 limit)
  - `NaiveDate` → `DATE` (all drivers)
  - `NaiveTime` → `TIME` (all drivers)

#### Code Organization
- **lib.rs**: Complete reorganization with detailed module documentation
  - Added module-level documentation
  - Organized imports and re-exports with descriptive comments
  - Added quick start example in documentation

- **query_builder.rs**: Enhanced with comprehensive documentation
  - Detailed documentation for all public methods
  - Added examples for UUID filtering and querying
  - Documented filter types and query building process
  - Added type-safe binding documentation

- **database.rs**: Improved with detailed connection and schema management docs
  - Documented all driver types and their differences
  - Added comprehensive examples for connection strings
  - Documented table creation and foreign key management
  - Explained SQL dialect differences across drivers

- **migration.rs**: Enhanced migration documentation
  - Documented two-phase migration approach
  - Added examples for complex migration scenarios
  - Explained task execution order
  - Added idempotency documentation

- **model.rs**: Complete trait and structure documentation
  - Documented `Model` trait with examples
  - Added `ColumnInfo` field-by-field documentation
  - Included manual implementation examples
  - Added comprehensive attribute documentation

- **errors.rs**: Improved error handling documentation
  - Documented all error variants with use cases
  - Added error handling examples
  - Included helper methods for error creation
  - Added test examples

- **types.rs** (macro): Enhanced type mapping documentation
  - Documented all supported type mappings
  - Added examples for each type conversion
  - Explained Option<T> handling
  - Documented UUID support in detail

- **derive_model.rs** (macro): Improved macro implementation docs
  - Documented macro expansion process
  - Added attribute parsing documentation
  - Explained generated code structure
  - Added comprehensive examples

- **lib.rs** (macro): Complete macro crate documentation
  - Added overview of macro system
  - Documented all supported attributes
  - Included complete usage examples
  - Added type support documentation

#### Bug Fixes
- Fixed unused `mut` warnings in `query_builder.rs`
- Fixed unused `Result` warnings for `.add()` calls in `temporal.rs` and `value_binding.rs`
- Converted doc comments to regular comments in match arms (following Rust conventions)
- Removed non-existent `bottle` member from workspace configuration
- Fixed DateTime conversion using generic `to_string()` instead of driver-specific formats

### Performance
- **Reduced conversion overhead**: Driver-specific formatting eliminates unnecessary parsing
- **PostgreSQL type casting**: Explicit casting improves query planning and execution
- **Optimized string formats**: Each driver receives the optimal format for its internal representation

---

## [0.1.1] - Previous Release

### Initial Features
- Basic ORM functionality
- PostgreSQL, MySQL, and SQLite support
- Fluent query builder
- Automatic migrations
- Foreign key support
- Basic type mapping

[Unreleased]: https://github.com/Murilinho145SG/bottle-orm/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Murilinho145SG/bottle-orm/releases/tag/v0.1.1