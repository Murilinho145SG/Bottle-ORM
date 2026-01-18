use syn::{GenericArgument, PathArguments, Type};

/// Maps Rust types to their corresponding SQL types.
///
/// Returns a tuple containing:
/// 1. The SQL type string (e.g., "TEXT", "INTEGER").
/// 2. A boolean indicating if the type is nullable (Option<T>).
pub fn rust_type_to_sql(ty: &Type) -> (String, bool) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();

            // Handle Option<T> for nullable columns
            if type_name == "Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        let (inner_sql_type, _ignored_nullable) = rust_type_to_sql(inner_ty);
                        return (inner_sql_type, true);
                    }
                }
            }

            match type_name.as_str() {
                "i32" => ("INTEGER".to_string(), false),
                "i64" => ("BIGINT".to_string(), false),
                "String" => ("TEXT".to_string(), false),
                "bool" => ("BOOLEAN".to_string(), false),
                "f64" => ("DOUBLE PRECISION".to_string(), false),
                "DateTime" => ("TIMESTAMPTZ".to_string(), false),
                "NaiveDateTime" => ("TIMESTAMP".to_string(), false),
                "NaiveDate" => ("DATE".to_string(), false),
                "NaiveTime" => ("TIME".to_string(), false),
                _ => ("TEXT".to_string(), false),
            }
        } else {
            ("TEXT".to_string(), false)
        }
    } else {
        ("TEXT".to_string(), false)
    }
}
