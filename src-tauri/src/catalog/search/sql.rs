//! Composition and execution of search SQL.
//!
//! Only static SQL fragments written in this crate are concatenated; every
//! value coming from a request is bound as a `?` parameter.

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

use super::FacetValue;

/// Counts the rows of `from` matching `conditions`.
pub fn count(
    connection: &Connection,
    from: &str,
    conditions: &Conditions,
) -> rusqlite::Result<u64> {
    let sql = format!("SELECT COUNT(*) FROM {from}{}", conditions.where_clause());
    let count: i64 = connection
        .prepare_cached(&sql)?
        .query_row(params_from_iter(conditions.params()), |row| row.get(0))?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Runs a facet query returning `(value, count)` rows; fetches one row more
/// than `limit` to report whether more values exist.
pub fn facet_values(
    connection: &Connection,
    sql: &str,
    params: &[Value],
    limit: u32,
) -> rusqlite::Result<(Vec<FacetValue>, bool)> {
    let mut statement = connection.prepare_cached(sql)?;
    let mut values = statement
        .query_map(params_from_iter(params), |row| {
            Ok(FacetValue {
                value: row.get(0)?,
                count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
            })
        })?
        .take(
            usize::try_from(limit)
                .unwrap_or(usize::MAX)
                .saturating_add(1),
        )
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_more = values.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    values.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    Ok((values, has_more))
}

/// `WHERE` conditions joined with `AND`, with their parameters in order.
#[derive(Debug, Clone, Default)]
pub struct Conditions {
    clauses: Vec<String>,
    params: Vec<Value>,
}

impl Conditions {
    pub fn push(&mut self, clause: &str, params: impl IntoIterator<Item = Value>) {
        self.clauses.push(clause.to_owned());
        self.params.extend(params);
    }

    /// `column IN (...)`; a `None` value selects `column IS NULL`.
    pub fn push_values(&mut self, column: &'static str, values: &[Option<String>]) {
        if values.is_empty() {
            return;
        }
        let present: Vec<Value> = values
            .iter()
            .flatten()
            .map(|value| Value::Text(value.clone()))
            .collect();
        let null = values.iter().any(Option::is_none);
        let mut alternatives = Vec::new();
        if !present.is_empty() {
            alternatives.push(format!("{column} IN ({})", placeholders(present.len())));
        }
        if null {
            alternatives.push(format!("{column} IS NULL"));
        }
        self.push(&format!("({})", alternatives.join(" OR ")), present);
    }

    /// `column = ?` for an optional boolean flag.
    pub fn push_flag(&mut self, column: &'static str, flag: Option<bool>) {
        if let Some(flag) = flag {
            self.push(&format!("{column} = ?"), [Value::Integer(i64::from(flag))]);
        }
    }

    /// `[NOT] EXISTS (subquery)` for an optional boolean flag.
    pub fn push_exists(&mut self, subquery: &'static str, flag: Option<bool>) {
        if let Some(flag) = flag {
            let operator = if flag { "EXISTS" } else { "NOT EXISTS" };
            self.push(&format!("{operator} ({subquery})"), []);
        }
    }

    /// `column [NOT] IN (subquery)` for an optional boolean flag.
    pub fn push_in(&mut self, column: &'static str, subquery: &'static str, flag: Option<bool>) {
        if let Some(flag) = flag {
            let operator = if flag { "IN" } else { "NOT IN" };
            self.push(&format!("{column} {operator} ({subquery})"), []);
        }
    }

    /// Adds a case-insensitive substring condition on a facet column.
    pub fn push_contains(&mut self, column: &'static str, search: Option<&str>) {
        if let Some(search) = search {
            self.push(
                &format!("{column} LIKE ? ESCAPE '\\'"),
                [Value::Text(format!("%{}%", escape_like(search)))],
            );
        }
    }

    pub fn where_clause(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.clauses.join(" AND "))
        }
    }

    pub fn params(&self) -> &[Value] {
        &self.params
    }
}

pub fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

pub fn text_values(values: &[String]) -> Vec<Value> {
    values
        .iter()
        .map(|value| Value::Text(value.clone()))
        .collect()
}

/// Escapes the `LIKE` wildcards so that the search is a literal substring.
fn escape_like(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// `ORDER BY` terms for a nullable column, keeping `NULL` values last in
/// both directions.
pub fn nulls_last(column: &str, descending: bool) -> String {
    let direction = if descending { " DESC" } else { "" };
    format!("{column} IS NULL, {column}{direction}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_parameterized_conditions() {
        let mut conditions = Conditions::default();
        conditions.push_values("m.year", &[Some("1980".to_owned()), None]);
        conditions.push_flag("m.is_bios", Some(false));
        conditions.push_exists("SELECT 1 FROM t", Some(false));

        assert_eq!(
            conditions.where_clause(),
            " WHERE (m.year IN (?) OR m.year IS NULL) AND m.is_bios = ? AND NOT EXISTS (SELECT 1 FROM t)"
        );
        assert_eq!(
            conditions.params(),
            [Value::Text("1980".to_owned()), Value::Integer(0)]
        );
    }

    #[test]
    fn ignores_unset_filters() {
        let mut conditions = Conditions::default();
        conditions.push_values("m.year", &[]);
        conditions.push_flag("m.is_bios", None);
        conditions.push_exists("SELECT 1", None);
        conditions.push_contains("m.year", None);

        assert_eq!(conditions.where_clause(), "");
        assert!(conditions.params().is_empty());
    }

    #[test]
    fn escapes_like_wildcards() {
        let mut conditions = Conditions::default();
        conditions.push_contains("m.manufacturer", Some("50%_off\\"));

        assert_eq!(
            conditions.params(),
            [Value::Text("%50\\%\\_off\\\\%".to_owned())]
        );
    }

    #[test]
    fn orders_nulls_last() {
        assert_eq!(nulls_last("m.year", false), "m.year IS NULL, m.year");
        assert_eq!(nulls_last("m.year", true), "m.year IS NULL, m.year DESC");
    }
}
