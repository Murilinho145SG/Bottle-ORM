# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.2] - 2026-03-03

### Added
- **DateTime Decoding Resilience**: Enhanced `FromAnyRow` for `DateTime<Utc>`, `NaiveDateTime`, `NaiveDate`, `NaiveTime`, and `Uuid` with a numeric (`i64`) fallback. This allows decoding from Unix timestamps in drivers that don't return ISO 8601 strings.
- **Scalar Tuple Regression Test**: Added `bottle-orm/tests/scalar_tuple_test.rs` to verify scalar queries with tuples containing `DateTime<Utc>`, specifically for PostgreSQL `Timestamptz`.

### Fixed
- **PostgreSQL Tuple Decoding**: Fixed a critical `ColumnDecode` error in PostgreSQL when using `scalar()` with tuples (e.g., `(String, DateTime<Utc>)`). The `select_args_sql` now correctly casts temporal types to JSON/Text even when the result type is a tuple or primitive.
- **Tuple/Primitive Index Consistency**: Standardized `FromAnyRow` implementations to use a common `from_any_row_at` pattern with an explicit index, ensuring reliable decoding for joined tables and multi-column results.

### Changed
- **Comprehensive API Documentation**: Significantly expanded the documentation across the core library, ensuring all public functions in `Database`, `DatabaseBuilder`, `RawQuery`, and `QueryBuilder` have clear descriptions, parameter/return type details, and practical examples.
- **Enhanced Example Style**: Standardized code examples in documentation to include `// SQL:` comments that illustrate the generated SQL query for each method, providing better visibility into the library's internal behavior.

## [0.5.1] - 2026-03-03

### Added
- **Resilient Decoding**: Enhanced `FromAnyRow` macro with a triple-fallback mechanism. It now tries to map columns using `table__column`, `column`, and `struct__column` patterns, making it extremely robust against DTO/Table naming mismatches.
- **Improved Join Resolution**: Refactored `select_args_sql` to automatically resolve and quote identifiers in manual selects and wildcard expansions, ensuring correct table prefixing even in complex JOIN scenarios.
- **Unified Query Execution**: Standardized `scan`, `scan_as`, `first`, and `scalar` to use a central `write_select_sql` method, ensuring consistent SQL generation and robust argument binding across all query types.
- **Advanced Query Features**: Completed implementation of `union`, `union_all`, and `filter_subquery` (WHERE IN subquery) with full support for argument propagation from inner queries.
- **Smart Alias Detection**: Enhanced `select_args_sql` to robustly handle manual aliases (e.g., `name AS display_name`), preventing double-AS syntax errors while maintaining compatibility with DTO mapping.
- **Extended Test Suite**: Added comprehensive integration tests for complex QueryBuilder scenarios including unions, subqueries, and multi-level aggregations.

### Fixed
- **Postgres Timestamptz Expansion**: Wildcard expansions (`*` or `table.*`) now correctly identify and cast temporal types to JSON in PostgreSQL, fixing decoding errors in the `Any` driver.
- **Column Collision Prevention**: Smart alias generation now detects potential name collisions in the result struct and uses qualified aliases only when necessary.
- **Infinite Loop Fix**: Resolved a critical deadlock caused by an infinite loop in raw SQL placeholder replacement.
- **Auto-Placeholder Injection**: Improved `where_raw` and `update_raw` to automatically append `?` or ` = ?` when a value is provided but the SQL string is incomplete.
- **Upsert Data Mapping**: Fixed a critical bug in `upsert` where column names and values were swapped in the data map, causing `ColumnDecode` errors in SQLite.
- **Compilation Warning Fixes**: Resolved multiple `unused_mut`, `unused_variable`, and `unused_import` warnings across the codebase.

## [0.5.0] - 2026-03-02

### Added
- **Raw Update Support**: Introduced `update_raw(col, expr, value)` in `QueryBuilder` to allow updates with SQL expressions (e.g., `SET age = age + 1`) and parameter binding.
- **Improved Soft Delete Logic**: Refactored internal update and delete operations to apply soft delete filters more consistently via `apply_soft_delete_filter()`.

## [0.4.27] - 2026-03-01
... rest of changelog
