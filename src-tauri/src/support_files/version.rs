//! Semantic ordering of MAME version strings, as written by support files.
//!
//! Support files use the historical MAME numbering: `0.1` to `0.288`, betas
//! (`0.37b5`), release candidates (`0.33rc1`), updates (`0.106u3`), letter
//! revisions (`0.26a`, `0.69b`) and a few oddities (`0.71u3p`). Sorting them
//! as text would put `0.10` before `0.9`, so they are compared by an integer
//! key instead: major, minor, stage (beta, release candidate, release, letter
//! revision, update), stage number and trailing letter.

/// Stage of a version within one minor number, in release order.
const STAGE_BETA: i64 = 0;
const STAGE_RELEASE_CANDIDATE: i64 = 1;
const STAGE_RELEASE: i64 = 2;
const STAGE_LETTER: i64 = 3;
const STAGE_UPDATE: i64 = 4;

const MAX_MAJOR: i64 = 99;
const MAX_MINOR: i64 = 99_999;
const MAX_NUMBER: i64 = 9_999;

/// The sort key of a MAME version, or `None` when the text is not a version
/// in the formats listed above. Keys compare like the versions they encode.
pub fn sort_key(version: &str) -> Option<i64> {
    let (major, rest) = version.trim().split_once('.')?;
    let major = number(major)?;
    let minor_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let minor = number(&rest[..minor_end])?;
    let (stage, stage_number, letter) = suffix(&rest[minor_end..])?;
    if major > MAX_MAJOR || minor > MAX_MINOR || stage_number > MAX_NUMBER {
        return None;
    }
    Some(
        ((((major * (MAX_MINOR + 1) + minor) * 10 + stage) * (MAX_NUMBER + 1) + stage_number)
            * 100)
            + letter,
    )
}

/// Whether `version` is a MAME version string.
pub fn is_version(version: &str) -> bool {
    sort_key(version).is_some()
}

/// `(stage, stage number, trailing letter)` of the text after the minor number.
fn suffix(suffix: &str) -> Option<(i64, i64, i64)> {
    if suffix.is_empty() {
        return Some((STAGE_RELEASE, 0, 0));
    }
    if let Some(rest) = suffix.strip_prefix("rc") {
        return Some((STAGE_RELEASE_CANDIDATE, number(rest)?, 0));
    }
    // `b` followed by digits is a beta; `b` alone is a letter revision (`0.69b`).
    if let Some(rest) = suffix.strip_prefix('b')
        && !rest.is_empty()
    {
        return Some((STAGE_BETA, number(rest)?, 0));
    }
    if let Some(rest) = suffix.strip_prefix('u') {
        let digits_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let update = number(&rest[..digits_end])?;
        let letter = match &rest[digits_end..] {
            "" => 0,
            tail => letter(tail)?,
        };
        return Some((STAGE_UPDATE, update, letter));
    }
    Some((STAGE_LETTER, 0, letter(suffix)?))
}

fn number(text: &str) -> Option<i64> {
    if text.is_empty() || text.len() > 6 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A single lowercase letter, `a` = 1.
fn letter(text: &str) -> Option<i64> {
    match text.as_bytes() {
        [byte @ b'a'..=b'z'] => Some(i64::from(byte - b'a') + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(versions: &[&str]) -> Vec<String> {
        let mut versions: Vec<&str> = versions.to_vec();
        versions.sort_by_key(|version| sort_key(version).expect(version));
        versions.into_iter().map(str::to_owned).collect()
    }

    #[test]
    fn orders_versions_semantically_not_as_text() {
        assert_eq!(
            sorted(&["0.10", "0.9", "0.100", "0.288", "0.1", "0.2"]),
            ["0.1", "0.2", "0.9", "0.10", "0.100", "0.288"]
        );
    }

    #[test]
    fn orders_betas_candidates_releases_letters_and_updates() {
        assert_eq!(
            sorted(&[
                "0.36", "0.35", "0.35rc2", "0.35b13", "0.35b2", "0.35rc1", "0.36b1", "0.34rc1",
            ]),
            [
                "0.34rc1", "0.35b2", "0.35b13", "0.35rc1", "0.35rc2", "0.35", "0.36b1", "0.36"
            ]
        );
        assert_eq!(
            sorted(&["0.69u3", "0.69b", "0.69", "0.69a", "0.70"]),
            ["0.69", "0.69a", "0.69b", "0.69u3", "0.70"]
        );
        assert_eq!(
            sorted(&["0.71u3p", "0.72", "0.71u3", "0.71u2", "0.106u10", "0.106u2"]),
            ["0.71u2", "0.71u3", "0.71u3p", "0.72", "0.106u2", "0.106u10"]
        );
    }

    #[test]
    fn rejects_text_that_is_not_a_version() {
        for text in [
            "", "0", "0.", "abc", "1.2.3", "0.37beta", "0.10x2", "0.1u", "v0.288", "0.37bb",
        ] {
            assert_eq!(sort_key(text), None, "{text}");
        }
        assert!(is_version(" 0.288 "));
    }
}
