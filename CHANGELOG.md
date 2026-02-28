# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.15] - 2026-02-28

### Fixed
- **Postgres Temporal Types in `scalar()`**: Fixed an issue where `DateTime` and other temporal types could not be decoded when using `scalar()` with the PostgreSQL driver. Added automatic `to_json` casting similar to `scan()`.

## [0.4.14] - 2026-02-28

### Added
- **Scalar Tuple Support**: Enhanced `scalar()` to support tuples (e.g., `(String, DateTime<Utc>)`), allowing multiple columns to be fetched in a single row result.
- **Unified FromAnyRow Trait**: Merged positional decoding logic into the main `FromAnyRow` trait with a new `from_any_row_at` method.
- **Positional Struct Decoding**: Structs derived with `Model` or `FromAnyRow` now automatically support positional decoding, enabling their use inside tuples (e.g., `scan::<(User, Profile)>()`).
- **Improved Temporal/UUID Decoding**: Enhanced positional decoding for `DateTime` and `Uuid` to handle both direct and `Option` types robustly.

### Fixed
- **Scalar Type Constraint**: Removed restrictive `sqlx::Type<Any>` constraint from `scalar()`, enabling it to work with any type that implements `FromAnyRow`.

## [0.4.13] - 2026-02-28

### Added
- **Full Alias Support**: Added comprehensive support for table aliases via `.alias("u")`.
- **Join Alias Tracking**: Added `join_aliases` to `QueryBuilder` to track and respect aliases used in JOINS.
- **Smart Prefixing**: Filters (`filter`, `is_null`, etc.) now automatically apply the correct table/alias prefix only if the column belongs to the main model, preventing ambiguity in multi-table queries.
- **Alias Integration Tests**: Added `tests/alias_test.rs` to verify alias functionality in filters, joins, and DTO mapping.

### Fixed
- **Multi-column Select Strings**: Improved `select()` to correctly handle strings with multiple columns (e.g., `.select("id, name")`) by processing each item individually.
- **DTO Mapping with Aliases**: Fixed an issue where using a table alias would break automatic DTO mapping. Result set columns now use the original table name in aliases (e.g., `AS user__id`) even when the table is aliased in SQL.
- **Ambiguous Column References**: Resolved issues in `tuple_join_test.rs` where columns from joined tables were incorrectly prefixed with the main table name.
- **Partial Move Errors**: Fixed Rust compiler errors related to partially moved `self.alias` in `scan`, `scan_as`, and `first` methods.

## [0.4.12] - 2026-02-27

### Fixed
- **Star Expansion in `scan_as`**: Fixed a bug where `user.*` or `*` in `.select()` would not expand columns, causing temporal types to fail on PostgreSQL because they weren't being cast to JSON.
- **Empty Select in `scan_as`**: Improved `scan_as` to automatically generate a correct SELECT clause based on the DTO fields when no `.select()` is called, including proper table names and temporal casting.
- **Table Name Resolution**: Fixed an issue where DTO table metadata could cause "table not found" errors when generating automatic SELECT clauses.

## [0.4.11] - 2026-02-27

### Fixed
- **PostgreSQL Temporal Types in `scan_as`**: Fixed a bug where temporal types (like `DateTime<Utc>`) could not be decoded when using `scan_as` with the PostgreSQL driver. Added automatic `to_json` casting and aliasing for these types.
- **DTO Mapping**: Improved alias handling in `scan_as` to ensure consistent mapping to DTO fields across all database drivers.

### Changed
- **`scan_as` Bound**: Now requires the target DTO to implement `AnyImpl` (automatically provided by `#[derive(FromAnyRow)]`) to support automatic type detection and casting.
- **Version Bump**: Updated all crates to `0.4.11`.

## [0.4.10] - 2026-02-27

### Added
- **`scan_as<R>`**: New method in `QueryBuilder` to map query results to custom DTOs that implement `FromAnyRow`. This is useful for complex queries and JOINs where the result does not map to a full `Model`.
- **`paginate_as<T, E, R>`**: New method in `Pagination` to execute paginated queries and map the results to a custom DTO.
- **Integration Tests**: Added `tests/scan_as_test.rs` to verify the new `scan_as` and `paginate_as` features.

### Changed
- **Documentation**: Added comprehensive documentation for `scan_as` and `paginate_as` methods.
- **Version Bump**: Updated all crates to `0.4.10`.

## [0.4.9] - 2026-02-27

### Fixed
- **Pagination Serialization**: Added `#[serde(skip_deserializing)]` to `max_limit` in the `Pagination` struct. This ensures that `max_limit` is always controlled by the application and cannot be overridden by user input during deserialization, enhancing security and predictability.

### Changed
- **Version Bump**: Updated all crates to `0.4.9`.

## [0.4.8] - 2026-02-25

### Added
- **Pagination `max_limit`**: Introduced `max_limit` field to `Pagination` struct to enforce a maximum number of items per page (default: 100).
- **Limit Enforcement**: Updated `Pagination::new` and `Pagination::apply` to automatically cap the `limit` at `max_limit`, defaulting to 10 if exceeded.

### Changed
- **Query Builder `select`**: Removed automatic `to_snake_case()` conversion in `QueryBuilder::select()`. This allows for more flexible column naming, especially when using aliases or raw SQL fragments in the select clause.
- **Version Bump**: Updated all crates to `0.4.8`.

## [0.4.7] - 2026-02-20

### Added

- **Composite Primary Key Support**: Enhanced `create_table` to support composite primary keys by collecting all primary key columns and defining them as a table-level constraint.
- **Improved Code Documentation**: Added comprehensive English comments to the core database management logic, specifically around schema generation and constraint handling.

### Fixed

- **Create Table Syntax**: Fixed `CREATE TABLE` query assembly to properly handle table constraints and ensure correct column definitions for primary keys across different database drivers.
- **NOT NULL Constraints**: Refined `NOT NULL` logic to ensure that primary key columns are always marked as non-nullable, even when not explicitly specified in the model.

## [0.4.6] - 2026-01-30

### Added

- **Tuple Query Support**: Added support for mapping query results directly to tuples of Models (e.g., `(User, Account)`). This enables single-query JOINs with automatic column aliasing (`user__id`, `account__id`) to avoid name collisions.
- **Trait `FromAnyRow`**: Introduced `FromAnyRow` trait to handle robust row mapping for complex types (tuples) and to replace `sqlx::FromRow` usage internally for better control over type conversions (especially `Uuid` and `DateTime` with `sqlx::Any`).
- **Field Constants**: Added auto-generated field constants module (e.g., `user_fields::AGE`) for Model structs to support autocomplete and safer query building (contribution by Marcos Brito).
- **Omit Attribute**: Added `#[orm(omit)]` attribute to exclude specific columns from being selected by default (contribution by Marcos Brito).

### Fixed

- **Postgres JSON Casting**: Restricted `to_json` casting for temporal types to PostgreSQL driver only, preventing syntax errors on other databases (contribution by Marcos Brito).
- **UUID/Time Decoding**: Improved reliability of `Uuid` and `DateTime` decoding on `sqlx::Any` driver by strictly using string parsing fallback, resolving "trait bound not satisfied" errors.

## [0.4.5] - 2026-01-27

### Added

- **Transaction Raw SQL**: Added `.raw()` method to `Transaction` struct, allowing raw SQL queries to be executed atomically within a transaction scope.
- **Enhanced Raw Query**: Added `fetch_optional()`, `fetch_scalar()`, and `fetch_scalar_optional()` to `RawQuery` for more flexible data retrieval.

## [0.4.4] - 2026-01-27

### Added

- **Raw SQL Support**: Introduced `db.raw("SELECT ...")` to allow executing arbitrary SQL queries with parameter binding (`.bind()`), mapping to structs (`.fetch_all()`, `.fetch_one()`), or executing updates (`.execute()`). This provides an escape hatch for complex queries not supported by the query builder.

## [0.4.3] - 2026-01-23

### Fixed

- **Conflicting Implementation**: Fixed `AnyImpl` conflict when deriving both `Model` and `FromAnyRow`.
- **Model Derive Enhancement**: `#[derive(Model)]` now automatically implements `sqlx::FromRow<'r, sqlx::any::AnyRow>`, removing the need for `FromAnyRow` or manual implementation. It robustly handles `DateTime` and `Uuid` decoding from `AnyRow` (supporting both text and binary protocols via string parsing fallback).
- **Dependency Features**: Added `uuid` feature to `sqlx` dependency in `bottle` crate (example) and `bottle-orm`.

## [0.4.2] - 2026-01-23

### Fixed

- **Pagination Compilation Error**: Fixed an issue where `#[derive(Model)]` did not implement `AnyImpl`, causing compilation errors when using `paginate()` or `scan()` with models. Now `derive(Model)` automatically implements `AnyImpl`.
- **SQLx UUID Feature**: Enabled `uuid` feature in `sqlx` dependency to ensure proper UUID handling in `Any` driver.

## [0.4.1] - 2026-01-23

### Added

- **Database Configuration**: Introduced `DatabaseBuilder` to allow custom connection pool settings.
  - Configure `max_connections`, `min_connections`, `acquire_timeout`, `idle_timeout`, and `max_lifetime`.

## [0.4.0] - 2026-01-23

### Features

#### 🚀 Enhanced Query Builder

- **Joins**: Added support for explicit joins: `left_join`, `right_join`, `inner_join`, `full_join`.
- **Grouping**: Added `group_by` and `having` methods for analytical queries.
- **Distinct**: Added `distinct()` method to filter duplicate rows.
- **Aggregates**: Added helper methods for `count()`, `sum()`, `avg()`, `min()`, and `max()`.

#### 🌐 Web Framework Integration

- **Pagination Module**: Introduced `bottle_orm::pagination` with `Pagination` and `Paginated<T>` structs.
  - Implements `Serialize`/`Deserialize` for easy integration with frameworks like **Axum** and **Actix-web**.
  - `paginate()` method automatically executes count and data queries in a single step.

#### 🛠️ Extended Type Support

- **Numeric Types**: Added support for `f32` (REAL), `u32` (INTEGER), `i16` (SMALLINT), `u16` (INTEGER), `i8`/`u8` (SMALLINT).
- **JSON Support**: Added first-class support for `serde_json::Value` (mapped to `JSONB` in Postgres).
- **Temporal Improvements**:
  - Added support for `DateTime<FixedOffset>` and `DateTime<Local>`.
  - Improved parsing resilience for various date string formats.

#### 💾 Database Compatibility

- **Foreign Keys**:
  - **SQLite**: Added support for inline foreign keys in `create_table` (since SQLite doesn't support `ADD CONSTRAINT`).
  - **MySQL**: Implemented `assign_foreign_keys` using `information_schema` checks.
  - **PostgreSQL**: Maintained existing support.

### Documentation

- **Comprehensive Docs**: Added detailed Rustdoc comments with examples for all public modules (`query_builder`, `pagination`, `transaction`, etc.).

## [0.3.4] - 2026-01-22

### Fixed

- **Lifetime "Implementation not general enough" Error**: Resolved a critical compilation error when using `QueryBuilder` methods (like `insert`, `update`, `first`, `scan`) in async contexts such as `axum` handlers.
  - This was caused by higher-ranked trait bounds (HRTB) on the `Connection` trait and implicit future lifetimes.
  - **Refactored `QueryBuilder`**: It now stores the `driver` explicitly and handles the connection generic `E` more flexibly.
  - **Explicit Future Lifetimes**: Async methods in `QueryBuilder` (`insert`, `update`, `updates`, `update_partial`, `execute_update`) now return `BoxFuture<'b, ...>` to explicitly bind the future's lifetime to the `self` borrow.
- **Connection Trait**: Simplified by removing the `driver()` method, reducing trait complexity.
- **Transaction**: Improved `Transaction` implementation to allow `&mut Transaction` to work seamlessly with `QueryBuilder`.

## [0.3.3] - 2026-01-22

### Fixed

- **Transaction Model Lifetime**: Resolved a critical lifetime issue in `Transaction::model` that prevented the ORM from being used effectively in async handlers (like Axum) due to "implementation is not general enough" errors.
  - `QueryBuilder` now takes ownership of the connection handle (`E`) instead of a mutable reference (`&mut E`).
  - This allows `Database` (cloned) and `&mut Transaction` to be used interchangeably without lifetime conflicts.

## [0.3.2] - 2026-01-22

### Fixed

- **Transaction Implementation**: Fixed a bug in `Transaction` implementation where `Connection` was implemented for `&mut Transaction` instead of `Transaction`, which caused issues with borrow checker and usage in `QueryBuilder`.
- **Connection Trait**: Added blanket implementation of `Connection` for `&'a mut T` where `T: Connection`, improving ergonomics.

## [0.3.1] - 2026-01-21

### Changed

- **Debug Mode Improvements**: Replaced `println!` with `log::debug!` for query logging.
  - Queries are now logged at the `DEBUG` level.
- **Foreign Key Validation**: Relaxed `Option<T>` requirement for fields annotated with `#[foreign_key]` to prepare for future eager loading features.
- **Documentation**: Added documentation for the `.debug()` method.

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
