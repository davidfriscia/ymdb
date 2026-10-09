//! Recognition of the version string printed by `mame -version`.
//!
//! MAME prints its build version as a single line in the format
//! `<release> (<revision>)`, for example `0.280 (mame0280)`:
//!
//! - `<release>` is the release number: two dot-separated numeric components.
//! - `<revision>` identifies the source revision of the build: `mame0280` for
//!   official releases, `mame0280-12-gabc1234` for development builds, or
//!   `unknown` for builds made outside a Git checkout.
//!
//! See `emulator_info::get_build_version()` and `src/version.cpp` in the MAME
//! sources. Recognition is deliberately strict so that arbitrary programs that
//! happen to exit successfully are not mistaken for MAME: the output must
//! consist of exactly one non-empty line in this format.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MameVersion {
    /// Release number, for example `0.280`.
    pub release: String,
    /// Source revision of the build, for example `mame0280`.
    pub revision: String,
}

/// Parses the standard output of `mame -version`.
pub fn parse_version_output(stdout: &str) -> Option<MameVersion> {
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }

    let (release, rest) = line.split_once(' ')?;
    let revision = rest.strip_prefix('(')?.strip_suffix(')')?;
    if !is_release_number(release) || !is_revision(revision) {
        return None;
    }

    Some(MameVersion {
        release: release.to_owned(),
        revision: revision.to_owned(),
    })
}

/// `<major>.<minor>`, both non-empty ASCII digit sequences.
fn is_release_number(value: &str) -> bool {
    let is_number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    value
        .split_once('.')
        .is_some_and(|(major, minor)| is_number(major) && is_number(minor))
}

/// A non-empty token made of characters used in Git-describe style revisions.
fn is_revision(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(release: &str, revision: &str) -> Option<MameVersion> {
        Some(MameVersion {
            release: release.to_owned(),
            revision: revision.to_owned(),
        })
    }

    #[test]
    fn parses_official_release_output() {
        // Exact bytes printed by MAME 0.280 on Windows.
        assert_eq!(
            parse_version_output("0.280 (mame0280)\r\n"),
            version("0.280", "mame0280")
        );
        assert_eq!(
            parse_version_output("0.240 (mame0240)"),
            version("0.240", "mame0240")
        );
    }

    #[test]
    fn parses_development_and_unversioned_builds() {
        assert_eq!(
            parse_version_output("0.281 (mame0280-512-g0123abcd)\n"),
            version("0.281", "mame0280-512-g0123abcd")
        );
        assert_eq!(
            parse_version_output("0.264 (unknown)\n"),
            version("0.264", "unknown")
        );
    }

    #[test]
    fn ignores_surrounding_whitespace_and_blank_lines() {
        assert_eq!(
            parse_version_output("\n  0.280 (mame0280)  \n\n"),
            version("0.280", "mame0280")
        );
    }

    #[test]
    fn rejects_output_not_produced_by_mame() {
        let invalid = [
            "",
            "\n\n",
            "Error: unknown option: -version",
            "0.280",
            "0.280 ()",
            "0.280 (mame 0280)",
            "0.280 (mame0280) extra",
            "0.280  (mame0280)",
            "0.280\t(mame0280)",
            "v0.280 (mame0280)",
            "0.280b (mame0280)",
            "1.2.3 (build)",
            "Python 3.12.1",
            "MAME v0.106 (Mar 21 2006)",
            "0.280 (mame0280)\nsecond line",
        ];
        for output in invalid {
            assert_eq!(parse_version_output(output), None, "accepted {output:?}");
        }
    }
}
