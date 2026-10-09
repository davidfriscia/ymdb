//! Conversion of user search text into an FTS5 query.
//!
//! Semantics:
//! - The text is split into words: maximal runs of Unicode letters and
//!   digits. Everything else (spaces, punctuation, `_`, symbols, FTS5
//!   operators such as `"`, `*`, `:`, `(`, `-`, `^`) only separates words.
//!   This matches how the `unicode61` tokenizer splits the indexed text.
//! - Every word must match (AND), in any indexed column, as a word prefix:
//!   `pac man` matches "Pac-Man", `street fight` matches "Street Fighter".
//! - Matching ignores case and diacritics (`attache` matches "Attaché").
//! - Each word is passed to FTS5 as a quoted string with a prefix operator
//!   (`"pac"*`), so user input can never use FTS5 syntax.
//! - Text without any letter or digit does not restrict the results.
//!
//! The trimmed, lowercased text is also kept to rank an exact short-name
//! match (`sf2`, `pacman`) first.

use super::SearchError;

/// Maximum number of words in a search.
pub const MAX_WORDS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextQuery {
    /// FTS5 MATCH expression, built only from quoted letter/digit words.
    pub match_expression: String,
    /// Trimmed, lowercased input, compared with short names.
    pub exact_name: String,
}

pub fn parse(text: Option<&str>) -> Result<Option<TextQuery>, SearchError> {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        return Ok(None);
    }
    if words.len() > MAX_WORDS {
        return Err(SearchError::InvalidRequest(format!(
            "search text must contain at most {MAX_WORDS} words"
        )));
    }
    let match_expression = words
        .iter()
        .map(|word| format!("\"{word}\"*"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Some(TextQuery {
        match_expression,
        exact_name: text.to_lowercase(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression(text: &str) -> Option<String> {
        parse(Some(text))
            .expect("valid")
            .map(|query| query.match_expression)
    }

    #[test]
    fn empty_or_symbol_only_text_does_not_restrict() {
        assert_eq!(parse(None).expect("none"), None);
        assert_eq!(expression(""), None);
        assert_eq!(expression("   \t "), None);
        assert_eq!(expression("\"*:-()^+"), None);
    }

    #[test]
    fn splits_words_and_quotes_each_as_prefix() {
        assert_eq!(expression("pacman").as_deref(), Some("\"pacman\"*"));
        assert_eq!(
            expression("  Street   Fighter ").as_deref(),
            Some("\"Street\"* \"Fighter\"*")
        );
        assert_eq!(expression("Pac-Man").as_deref(), Some("\"Pac\"* \"Man\"*"));
        assert_eq!(
            expression("neogeo_x").as_deref(),
            Some("\"neogeo\"* \"x\"*")
        );
    }

    #[test]
    fn neutralizes_fts5_syntax() {
        assert_eq!(
            expression("\"a\" OR b* NOT c NEAR(d) col:e ^f").as_deref(),
            Some("\"a\"* \"OR\"* \"b\"* \"NOT\"* \"c\"* \"NEAR\"* \"d\"* \"col\"* \"e\"* \"f\"*")
        );
        assert_eq!(
            expression("'; DROP TABLE machines; --").as_deref(),
            Some("\"DROP\"* \"TABLE\"* \"machines\"*")
        );
    }

    #[test]
    fn keeps_unicode_letters_and_digits() {
        assert_eq!(
            expression("Attaché 8:16").as_deref(),
            Some("\"Attaché\"* \"8\"* \"16\"*")
        );
        assert_eq!(expression("ストリート").as_deref(), Some("\"ストリート\"*"));
    }

    #[test]
    fn keeps_exact_name_for_ranking() {
        let query = parse(Some("  SF2CE ")).expect("valid").expect("query");

        assert_eq!(query.exact_name, "sf2ce");
    }

    #[test]
    fn rejects_too_many_words() {
        let text = vec!["w"; MAX_WORDS + 1].join(" ");

        assert!(matches!(
            parse(Some(&text)),
            Err(SearchError::InvalidRequest(_))
        ));
    }
}
