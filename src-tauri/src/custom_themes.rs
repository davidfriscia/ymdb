//! Custom themes (`custom-themes.json`).
//!
//! A custom theme is a named, independent set of design token values,
//! global to YMDB. Built-in themes are defined by the frontend stylesheet and
//! never stored here; a clone of any theme is a full copy of its values, so
//! later changes of the source never affect it.
//!
//! The tokens are defined by the frontend (`src/app/theme/themeTokens.ts`),
//! which validates them before saving and whenever it loads a theme, and only
//! applies them through generated CSS custom properties. This module stores
//! them verbatim with their format version (`tokensVersion`), so tokens
//! written by a newer YMDB are preserved, never rewritten. The file itself
//! has a schema version: a file of a newer version, or an invalid file, is
//! reported and never replaced.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file;

pub const CUSTOM_THEMES_FILE_NAME: &str = "custom-themes.json";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_THEMES: usize = 200;
pub const MAX_NAME_LENGTH: usize = 60;
/// Largest serialized tokens of one theme.
pub const MAX_TOKENS_SIZE: usize = 16 * 1024;
const MAX_SOURCE_ID_LENGTH: usize = 80;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomThemes {
    pub schema_version: u32,
    pub themes: Vec<CustomTheme>,
}

impl Default for CustomThemes {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            themes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomTheme {
    /// Opaque and immutable: renaming never changes it.
    pub id: String,
    pub name: String,
    /// The theme this one was cloned from, for information only: a clone
    /// never follows later changes of its source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_theme_id: Option<String>,
    /// Version of the tokens format, defined by the frontend.
    pub tokens_version: u32,
    /// The tokens, a JSON object kept verbatim.
    pub tokens: serde_json::Value,
    /// Milliseconds since the Unix epoch.
    pub created_at: u64,
    pub updated_at: u64,
}

/// A theme to create, from a clone or an imported file.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTheme {
    pub name: String,
    pub source_theme_id: Option<String>,
    pub tokens_version: u32,
    pub tokens: serde_json::Value,
}

/// Whether `id` has the form of a custom theme id (32 lowercase hex digits).
pub fn is_theme_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl CustomThemes {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "schema version {} is not supported (expected {SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        if self.themes.len() > MAX_THEMES {
            return Err(format!("at most {MAX_THEMES} custom themes are supported"));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for theme in &self.themes {
            if !is_theme_id(&theme.id) {
                return Err(format!("invalid custom theme id '{}'", theme.id));
            }
            if !ids.insert(theme.id.as_str()) {
                return Err(format!("duplicate custom theme id '{}'", theme.id));
            }
            validate_name(&theme.name)?;
            if !names.insert(name_key(&theme.name)) {
                return Err(format!("duplicate custom theme name '{}'", theme.name));
            }
            validate_source(theme.source_theme_id.as_deref())?;
            validate_tokens(&theme.tokens)?;
        }
        Ok(())
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut CustomTheme, String> {
        self.themes
            .iter_mut()
            .find(|theme| theme.id == id)
            .ok_or_else(|| "this custom theme no longer exists".to_owned())
    }

    fn check_unique_name(&self, name: &str, except: Option<&str>) -> Result<(), String> {
        let key = name_key(name);
        let taken = self
            .themes
            .iter()
            .any(|theme| Some(theme.id.as_str()) != except && name_key(&theme.name) == key);
        if taken {
            Err(format!("a theme named '{name}' already exists"))
        } else {
            Ok(())
        }
    }

    /// Adds a theme; returns its id. An existing theme is never replaced.
    pub fn create(&mut self, theme: NewTheme, now: u64) -> Result<String, String> {
        let name = theme.name.trim();
        validate_name(name)?;
        validate_source(theme.source_theme_id.as_deref())?;
        validate_tokens(&theme.tokens)?;
        self.check_unique_name(name, None)?;
        if self.themes.len() >= MAX_THEMES {
            return Err(format!("at most {MAX_THEMES} custom themes are supported"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.themes.push(CustomTheme {
            id: id.clone(),
            name: name.to_owned(),
            source_theme_id: theme.source_theme_id,
            tokens_version: theme.tokens_version,
            tokens: theme.tokens,
            created_at: now,
            updated_at: now,
        });
        Ok(id)
    }

    /// Replaces the name and the tokens of a theme.
    pub fn update(
        &mut self,
        id: &str,
        name: &str,
        tokens_version: u32,
        tokens: serde_json::Value,
        now: u64,
    ) -> Result<(), String> {
        let name = name.trim();
        validate_name(name)?;
        validate_tokens(&tokens)?;
        self.check_unique_name(name, Some(id))?;
        let theme = self.get_mut(id)?;
        theme.name = name.to_owned();
        theme.tokens_version = tokens_version;
        theme.tokens = tokens;
        theme.updated_at = now;
        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: &str, now: u64) -> Result<(), String> {
        let name = name.trim();
        validate_name(name)?;
        self.check_unique_name(name, Some(id))?;
        let theme = self.get_mut(id)?;
        theme.name = name.to_owned();
        theme.updated_at = now;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let before = self.themes.len();
        self.themes.retain(|theme| theme.id != id);
        if self.themes.len() == before {
            return Err("this custom theme no longer exists".to_owned());
        }
        Ok(())
    }
}

/// Names are unique ignoring case and surrounding spaces.
fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn validate_name(name: &str) -> Result<(), String> {
    let length = name.chars().count();
    if name.trim() != name || length == 0 || length > MAX_NAME_LENGTH {
        return Err(format!(
            "a theme name must have 1 to {MAX_NAME_LENGTH} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("a theme name cannot contain control characters".to_owned());
    }
    Ok(())
}

fn validate_source(source: Option<&str>) -> Result<(), String> {
    match source {
        Some(source)
            if source.is_empty()
                || source.len() > MAX_SOURCE_ID_LENGTH
                || !source
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b':') =>
        {
            Err(format!("invalid source theme id '{source}'"))
        }
        _ => Ok(()),
    }
}

fn validate_tokens(tokens: &serde_json::Value) -> Result<(), String> {
    if !tokens.is_object() {
        return Err("theme tokens must be an object".to_owned());
    }
    if tokens.to_string().len() > MAX_TOKENS_SIZE {
        return Err(format!(
            "theme tokens must be at most {MAX_TOKENS_SIZE} bytes"
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum CustomThemesStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for CustomThemesStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the custom themes file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for CustomThemesStoreError {}

#[derive(Debug, Clone)]
pub struct CustomThemesStore {
    file: PathBuf,
}

impl CustomThemesStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(CUSTOM_THEMES_FILE_NAME),
        }
    }

    /// Loads the themes; a missing file means none (nothing is written).
    pub fn load(&self) -> Result<CustomThemes, CustomThemesStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CustomThemes::default());
            }
            Err(source) => {
                return Err(CustomThemesStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| CustomThemesStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let themes: CustomThemes =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        themes.validate().map_err(invalid)?;
        Ok(themes)
    }

    pub fn save(&self, themes: &CustomThemes) -> Result<(), CustomThemesStoreError> {
        let invalid = |message: String| CustomThemesStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        themes.validate().map_err(invalid)?;
        let mut content =
            serde_json::to_vec_pretty(themes).map_err(|error| invalid(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| CustomThemesStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }
}

/// Largest theme file accepted by an import.
pub const MAX_THEME_FILE_SIZE: u64 = 64 * 1024;

/// Reads a theme file chosen for an import: a JSON object of bounded size.
/// Its format is checked by the frontend, which owns the tokens.
pub fn read_theme_file(path: &Path) -> Result<serde_json::Value, String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))?
        .len();
    if size > MAX_THEME_FILE_SIZE {
        return Err(format!(
            "a theme file must be at most {} KiB",
            MAX_THEME_FILE_SIZE / 1024
        ));
    }
    let content =
        fs::read(path).map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&content).map_err(|_| "the file is not valid JSON".to_owned())?;
    if !value.is_object() {
        return Err("the file is not a YMDB theme".to_owned());
    }
    Ok(value)
}

/// Serializes an exported theme document, refusing anything that is not a
/// bounded JSON object.
pub fn theme_file_content(document: &serde_json::Value) -> Result<Vec<u8>, String> {
    if !document.is_object() {
        return Err("a theme document must be an object".to_owned());
    }
    let mut content = serde_json::to_vec_pretty(document).map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_THEME_FILE_SIZE {
        return Err("the theme document is too large".to_owned());
    }
    content.push(b'\n');
    Ok(content)
}

/// A file name for an exported theme: the theme name reduced to letters,
/// digits and dashes, with the `.ymdb-theme.json` extension.
pub fn export_file_name(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "theme" } else { slug };
    format!("{slug}.ymdb-theme.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn new_theme(name: &str) -> NewTheme {
        NewTheme {
            name: name.to_owned(),
            source_theme_id: Some("neon80s".to_owned()),
            tokens_version: 1,
            tokens: json!({ "colorScheme": "dark", "colors": { "accent": "#ff27d8" } }),
        }
    }

    #[test]
    fn creates_updates_renames_and_deletes_themes() {
        let mut themes = CustomThemes::default();
        let id = themes.create(new_theme(" Midnight "), 10).expect("create");
        assert!(is_theme_id(&id));
        let theme = &themes.themes[0];
        assert_eq!(theme.name, "Midnight");
        assert_eq!(theme.source_theme_id.as_deref(), Some("neon80s"));
        assert_eq!((theme.created_at, theme.updated_at), (10, 10));

        themes
            .update(&id, "Midnight blue", 1, json!({ "colors": {} }), 20)
            .expect("update");
        themes.rename(&id, "Dusk", 30).expect("rename");
        let theme = &themes.themes[0];
        // The id never changes.
        assert_eq!(theme.id, id);
        assert_eq!(theme.name, "Dusk");
        assert_eq!(theme.tokens, json!({ "colors": {} }));
        assert_eq!((theme.created_at, theme.updated_at), (10, 30));

        themes.delete(&id).expect("delete");
        assert!(themes.themes.is_empty());
        assert!(themes.delete(&id).is_err());
        assert!(themes.rename(&id, "Gone", 40).is_err());
        assert!(themes.update(&id, "Gone", 1, json!({}), 40).is_err());
    }

    #[test]
    fn refuses_duplicate_names_without_replacing_themes() {
        let mut themes = CustomThemes::default();
        let id = themes.create(new_theme("Arcade"), 1).expect("create");
        assert!(themes.create(new_theme("arcade "), 2).is_err());
        assert_eq!(themes.themes.len(), 1);
        let other = themes.create(new_theme("Other"), 3).expect("create");
        assert!(themes.rename(&other, "ARCADE", 4).is_err());
        assert!(themes.update(&other, "Arcade", 1, json!({}), 4).is_err());
        // Its own name with another case is allowed.
        themes.rename(&id, "ARCADE", 5).expect("rename");
    }

    #[test]
    fn rejects_invalid_names_sources_and_tokens() {
        let mut themes = CustomThemes::default();
        for name in ["", "   ", "Tab\tname", &"x".repeat(MAX_NAME_LENGTH + 1)] {
            assert!(themes.create(new_theme(name), 1).is_err(), "{name:?}");
        }
        let mut theme = new_theme("Bad source");
        theme.source_theme_id = Some("../etc".to_owned());
        assert!(themes.create(theme, 1).is_err());
        let mut theme = new_theme("Not an object");
        theme.tokens = json!(["#ffffff"]);
        assert!(themes.create(theme, 1).is_err());
        let mut theme = new_theme("Large");
        theme.tokens = json!({ "x": "x".repeat(MAX_TOKENS_SIZE) });
        assert!(themes.create(theme, 1).is_err());
        assert!(themes.themes.is_empty());
    }

    #[test]
    fn persists_themes_and_keeps_unknown_token_versions() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CustomThemesStore::in_dir(dir.path());
        assert_eq!(store.load().expect("missing file"), CustomThemes::default());
        assert!(!dir.path().join(CUSTOM_THEMES_FILE_NAME).exists());

        let mut themes = CustomThemes::default();
        let future = json!({ "colors": { "newToken": "#123456" }, "deep": [1, 2] });
        themes
            .create(
                NewTheme {
                    name: "Future".to_owned(),
                    source_theme_id: None,
                    tokens_version: 9,
                    tokens: future.clone(),
                },
                1,
            )
            .expect("create");
        store.save(&themes).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded, themes);
        assert_eq!(loaded.themes[0].tokens_version, 9);
        assert_eq!(loaded.themes[0].tokens, future);
        // No source is written when there is none.
        let content = fs::read_to_string(dir.path().join(CUSTOM_THEMES_FILE_NAME)).expect("read");
        assert!(!content.contains("sourceThemeId"));
    }

    #[test]
    fn serializes_camel_case_fields() {
        let mut themes = CustomThemes::default();
        themes.create(new_theme("Serialized"), 5).expect("create");
        let value = serde_json::to_value(&themes).expect("serialize");
        assert_eq!(value["schemaVersion"], 1);
        let theme = &value["themes"][0];
        assert_eq!(theme["sourceThemeId"], "neon80s");
        assert_eq!(theme["tokensVersion"], 1);
        assert_eq!(theme["createdAt"], 5);
        assert_eq!(theme["updatedAt"], 5);
    }

    #[test]
    fn reports_invalid_files_and_never_replaces_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CustomThemesStore::in_dir(dir.path());
        let path = dir.path().join(CUSTOM_THEMES_FILE_NAME);
        let id = "0123456789abcdef0123456789abcdef";
        for content in [
            "{ not json".to_owned(),
            r#"{ "schemaVersion": 2, "themes": [] }"#.to_owned(),
            r#"{ "schemaVersion": 1, "themes": [], "extra": true }"#.to_owned(),
            format!(
                r#"{{ "schemaVersion": 1, "themes": [{{ "id": "{id}", "name": "A", "tokensVersion": 1, "tokens": {{}}, "createdAt": 1, "updatedAt": 1 }}, {{ "id": "{id}", "name": "B", "tokensVersion": 1, "tokens": {{}}, "createdAt": 1, "updatedAt": 1 }}] }}"#
            ),
            r#"{ "schemaVersion": 1, "themes": [{ "id": "NOT-AN-ID", "name": "A", "tokensVersion": 1, "tokens": {}, "createdAt": 1, "updatedAt": 1 }] }"#.to_owned(),
        ] {
            fs::write(&path, &content).expect("write");
            assert!(matches!(
                store.load(),
                Err(CustomThemesStoreError::Invalid { .. })
            ));
            assert_eq!(fs::read_to_string(&path).expect("read"), content);
        }
    }

    #[test]
    fn reads_theme_files_of_bounded_size() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("a.ymdb-theme.json");
        fs::write(&path, r#"{ "format": "ymdb-theme" }"#).expect("write");
        assert_eq!(
            read_theme_file(&path).expect("read"),
            json!({ "format": "ymdb-theme" })
        );
        fs::write(&path, "[1, 2]").expect("write");
        assert!(read_theme_file(&path).is_err());
        fs::write(&path, "{ broken").expect("write");
        assert!(read_theme_file(&path).is_err());
        fs::write(&path, " ".repeat(MAX_THEME_FILE_SIZE as usize + 1)).expect("write");
        assert!(read_theme_file(&path).unwrap_err().contains("at most"));
    }

    #[test]
    fn builds_export_contents_and_file_names() {
        assert!(theme_file_content(&json!("text")).is_err());
        let content = theme_file_content(&json!({ "format": "ymdb-theme" })).expect("content");
        assert!(content.ends_with(b"\n"));
        let large = json!({ "x": "x".repeat(MAX_THEME_FILE_SIZE as usize) });
        assert!(theme_file_content(&large).is_err());

        assert_eq!(export_file_name("Neon 80s"), "neon-80s.ymdb-theme.json");
        assert_eq!(
            export_file_name("  ../Mid:night!  "),
            "mid-night.ymdb-theme.json"
        );
        assert_eq!(export_file_name("***"), "theme.ymdb-theme.json");
    }
}
