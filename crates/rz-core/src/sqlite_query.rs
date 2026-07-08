use sqlx::{QueryBuilder, Sqlite, SqlitePool, sqlite::SqliteRow};

use crate::error::CoreError;

/// Apply a case-insensitive LIKE filter when the value is present and non-empty.
pub fn push_ilike(query_builder: &mut QueryBuilder<Sqlite>, column: &str, value: Option<&str>) {
    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            query_builder
                .push(" AND ")
                .push("LOWER(")
                .push(column)
                .push(") LIKE ")
                .push_bind(format!("%{}%", value.to_lowercase()));
        }
    }
}

/// Apply an equality filter when the value is present.
pub fn push_eq<T>(query_builder: &mut QueryBuilder<Sqlite>, column: &str, value: Option<T>)
where
    T: for<'q> sqlx::Encode<'q, Sqlite> + sqlx::Type<Sqlite>,
{
    if let Some(value) = value {
        query_builder
            .push(" AND ")
            .push(column)
            .push(" = ")
            .push_bind(value);
    }
}

/// Parse an optional `i16` filter from query text.
///
/// `default_on_empty` is used when the value is missing or empty.
/// `all` always disables the filter.
pub fn parse_optional_i16_filter(
    value: Option<&str>,
    field_name: &str,
    default_on_empty: Option<i16>,
) -> Result<Option<i16>, CoreError> {
    match value.map(str::trim) {
        None | Some("") => Ok(default_on_empty),
        Some("all") => Ok(None),
        Some(raw) => raw
            .parse::<i16>()
            .map(Some)
            .map_err(|_| CoreError::InvalidInput(format!("Invalid {} value: {}", field_name, raw))),
    }
}

/// Count rows for a filtered SQLite query.
pub async fn count_with_filters<F>(
    pool: &SqlitePool,
    base_sql: &'static str,
    apply_filters: F,
) -> Result<i64, CoreError>
where
    F: FnOnce(&mut QueryBuilder<Sqlite>),
{
    let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(base_sql);
    apply_filters(&mut query_builder);

    let count: (i64,) = query_builder.build_query_as().fetch_one(pool).await?;

    Ok(count.0)
}

/// Fetch rows for a filtered SQLite query with optional ordering and pagination.
pub async fn fetch_with_filters<T, F>(
    pool: &SqlitePool,
    base_sql: &'static str,
    apply_filters: F,
    order_by: Option<&'static str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<T>, CoreError>
where
    F: FnOnce(&mut QueryBuilder<Sqlite>),
    T: for<'r> sqlx::FromRow<'r, SqliteRow> + Send + Unpin,
{
    let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(base_sql);
    apply_filters(&mut query_builder);

    if let Some(order_by) = order_by {
        query_builder.push(" ORDER BY ").push(order_by);
    }

    if let Some(limit) = limit {
        query_builder.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = offset {
        query_builder.push(" OFFSET ").push_bind(offset);
    }

    let rows = query_builder.build_query_as::<T>().fetch_all(pool).await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::parse_optional_i16_filter;

    #[test]
    fn parse_optional_i16_filter_uses_default_when_empty() {
        let value = parse_optional_i16_filter(Some(""), "status", Some(1)).unwrap();

        assert_eq!(value, Some(1));
    }

    #[test]
    fn parse_optional_i16_filter_disables_filter_for_all() {
        let value = parse_optional_i16_filter(Some("all"), "status", Some(1)).unwrap();

        assert_eq!(value, None);
    }

    #[test]
    fn parse_optional_i16_filter_rejects_invalid_number() {
        let error = parse_optional_i16_filter(Some("active"), "status", None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid input: Invalid status value: active"
        );
    }
}
