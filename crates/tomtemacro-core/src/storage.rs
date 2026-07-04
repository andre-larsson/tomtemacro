//! Macro persistence in the per-platform data directory.
//!
//! The current format is `.tomte` — plain macro-script text (see the
//! `script` module). Legacy `.ron` files from the flat-event era keep
//! loading forever and convert to `.tomte` when saved from the editor.

use std::path::{Path, PathBuf};

use crate::model::{MacroFile, SCHEMA_VERSION};
use crate::script::{self, Script};

/// Extension of macro-script files.
pub const SCRIPT_EXT: &str = "tomte";

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("could not determine a data directory for this platform")]
    NoDataDir,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a valid macro file: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(
        "{path} uses schema version {found}, but this build only understands \
         up to {SCHEMA_VERSION} — update TomteMacro"
    )]
    UnsupportedVersion { path: PathBuf, found: u32 },
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> StorageError + '_ {
    move |source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Load a macro from any path, with schema-version checking.
pub fn load(path: &Path) -> Result<MacroFile, StorageError> {
    let text = std::fs::read_to_string(path).map_err(io_err(path))?;
    let file: MacroFile = ron::from_str(&text).map_err(|e| StorageError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    if file.version > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: file.version,
        });
    }
    Ok(file)
}

/// Load a macro in either format as a script. `.ron` files go through the
/// legacy loader and convert; anything else parses as macro-script text.
/// A script with no `# name:` directive is named after the file stem.
pub fn load_script(path: &Path) -> Result<Script, StorageError> {
    let legacy = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ron"));
    let mut loaded = if legacy {
        Script::from_macro_file(&load(path)?)
    } else {
        let text = std::fs::read_to_string(path).map_err(io_err(path))?;
        script::parse(&text).map_err(|e| StorageError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?
    };
    if loaded.meta.name.is_empty() {
        loaded.meta.name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(loaded)
}

/// Write macro-script text verbatim to an exact path — the editor's buffer
/// is the source of truth, so no reformatting happens on save.
pub fn save_text(text: &str, path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    std::fs::write(path, text).map_err(io_err(path))
}

/// Write a macro to an exact path (pretty RON with a header comment).
pub fn save(macro_file: &MacroFile, path: &Path) -> Result<(), StorageError> {
    let body =
        ron::ser::to_string_pretty(macro_file, ron::ser::PrettyConfig::default()).map_err(|e| {
            StorageError::Parse {
                path: path.to_path_buf(),
                message: e.to_string(),
            }
        })?;
    let text = format!("// TomteMacro macro file — edit freely, RON syntax\n{body}\n");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    std::fs::write(path, text).map_err(io_err(path))
}

/// The user's macro library: a directory of `.ron` files.
pub struct MacroStore {
    dir: PathBuf,
}

impl MacroStore {
    /// Platform-default location, e.g. `~/.local/share/tomtemacro/macros`.
    pub fn open_default() -> Result<Self, StorageError> {
        let dirs =
            directories::ProjectDirs::from("", "", "tomtemacro").ok_or(StorageError::NoDataDir)?;
        Ok(Self::at(dirs.data_dir().join("macros")))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// All macro files in the library (both formats), sorted by file name.
    pub fn list(&self) -> Result<Vec<PathBuf>, StorageError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&self.dir).map_err(io_err(&self.dir))?;
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|ext| ext == SCRIPT_EXT || ext == "ron")
            })
            .collect();
        paths.sort();
        Ok(paths)
    }

    /// Save script text under a slug derived from `name`, never overwriting
    /// an existing macro (appends `-2`, `-3`, … instead).
    pub fn save_new_script(&self, name: &str, text: &str) -> Result<PathBuf, StorageError> {
        let slug = slugify(name);
        let mut path = self.dir.join(format!("{slug}.{SCRIPT_EXT}"));
        let mut n = 1u32;
        while path.exists() {
            n += 1;
            path = self.dir.join(format!("{slug}-{n}.{SCRIPT_EXT}"));
        }
        save_text(text, &path)?;
        Ok(path)
    }

    /// Save under a slug derived from `meta.name`, never overwriting an
    /// existing macro (appends `-2`, `-3`, … instead).
    pub fn save_new(&self, macro_file: &MacroFile) -> Result<PathBuf, StorageError> {
        let slug = slugify(&macro_file.meta.name);
        let mut path = self.dir.join(format!("{slug}.ron"));
        let mut n = 1u32;
        while path.exists() {
            n += 1;
            path = self.dir.join(format!("{slug}-{n}.ron"));
        }
        save(macro_file, &path)?;
        Ok(path)
    }

    /// Rename both the file and the embedded name (RON `meta.name` or the
    /// script's `# name:` header directive).
    pub fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf, StorageError> {
        if path.extension().is_some_and(|ext| ext == SCRIPT_EXT) {
            let text = std::fs::read_to_string(path).map_err(io_err(path))?;
            let text = script::with_header_name(&text, new_name);
            let new_path = self.dir.join(format!("{}.{SCRIPT_EXT}", slugify(new_name)));
            save_text(&text, &new_path)?;
            if new_path != path {
                std::fs::remove_file(path).map_err(io_err(path))?;
            }
            return Ok(new_path);
        }
        let mut file = load(path)?;
        file.meta.name = new_name.to_string();
        let new_path = self.dir.join(format!("{}.ron", slugify(new_name)));
        if new_path != path {
            save(&file, &new_path)?;
            std::fs::remove_file(path).map_err(io_err(path))?;
            Ok(new_path)
        } else {
            save(&file, path)?;
            Ok(path.to_path_buf())
        }
    }

    pub fn delete(&self, path: &Path) -> Result<(), StorageError> {
        std::fs::remove_file(path).map_err(io_err(path))
    }
}

/// File-name-safe slug: lowercase alphanumerics (unicode kept) with runs of
/// anything else collapsed to single hyphens.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = true; // suppress leading dash
    for c in name.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("unnamed");
    }
    slug
}

/// Current time as an RFC 3339 UTC string for macro metadata.
pub fn now_utc_rfc3339() -> String {
    // Civil-from-days (Howard Hinnant's algorithm) — avoids a date-time
    // dependency for one timestamp.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (h, m, s) = (secs % 86_400 / 3600, secs % 3600 / 60, secs % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, MacroEvent, MacroMeta, MouseButton};

    fn temp_store(tag: &str) -> MacroStore {
        let dir =
            std::env::temp_dir().join(format!("tomtemacro-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        MacroStore::at(dir)
    }

    fn sample(name: &str) -> MacroFile {
        MacroFile::new(
            MacroMeta {
                name: name.into(),
                created_utc: now_utc_rfc3339(),
                os: "linux-x11".into(),
                screen: None,
                notes: String::new(),
            },
            vec![MacroEvent {
                delay_us: 0,
                kind: EventKind::ButtonPress(MouseButton::Left),
            }],
        )
    }

    #[test]
    fn save_load_list_delete_round_trip() {
        let store = temp_store("crud");
        let path = store.save_new(&sample("Test Macro!")).unwrap();
        assert_eq!(path.file_name().unwrap(), "test-macro.ron");

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.meta.name, "Test Macro!");
        assert_eq!(store.list().unwrap(), vec![path.clone()]);

        store.delete(&path).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn saving_same_name_never_overwrites() {
        let store = temp_store("dupes");
        let first = store.save_new(&sample("loop")).unwrap();
        let second = store.save_new(&sample("loop")).unwrap();
        assert_ne!(first, second);
        assert_eq!(second.file_name().unwrap(), "loop-2.ron");
    }

    #[test]
    fn rename_updates_file_and_meta() {
        let store = temp_store("rename");
        let path = store.save_new(&sample("before")).unwrap();
        let renamed = store.rename(&path, "after words").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "after-words.ron");
        assert!(!path.exists());
        assert_eq!(load(&renamed).unwrap().meta.name, "after words");
    }

    #[test]
    fn newer_schema_versions_are_rejected() {
        let store = temp_store("version");
        let mut file = sample("future");
        file.version = SCHEMA_VERSION + 1;
        let path = store.dir().join("future.ron");
        save(&file, &path).unwrap();
        assert!(matches!(
            load(&path),
            Err(StorageError::UnsupportedVersion { found, .. }) if found == SCHEMA_VERSION + 1
        ));
    }

    #[test]
    fn malformed_files_error_cleanly() {
        let store = temp_store("malformed");
        std::fs::create_dir_all(store.dir()).unwrap();
        let path = store.dir().join("broken.ron");
        std::fs::write(&path, "(this is not : valid").unwrap();
        assert!(matches!(load(&path), Err(StorageError::Parse { .. })));
    }

    #[test]
    fn tomte_files_save_load_list_and_rename() {
        let store = temp_store("tomte");
        let text = "# tomte-macro v1\nclick left\nwait 100ms\n";
        let path = store.save_new_script("Farm Loop!", text).unwrap();
        assert_eq!(path.file_name().unwrap(), "farm-loop.tomte");

        // No name directive → named after the file stem.
        let loaded = load_script(&path).unwrap();
        assert_eq!(loaded.meta.name, "farm-loop");
        assert_eq!(loaded.stats().instructions, 2);

        // Uniquing, listing, renaming.
        let second = store.save_new_script("Farm Loop!", text).unwrap();
        assert_eq!(second.file_name().unwrap(), "farm-loop-2.tomte");
        assert_eq!(store.list().unwrap().len(), 2);

        let renamed = store.rename(&path, "harvest").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "harvest.tomte");
        assert!(!path.exists());
        let text_after = std::fs::read_to_string(&renamed).unwrap();
        assert!(text_after.contains("# name: harvest"), "{text_after}");
        assert!(text_after.contains("click left"));
        assert_eq!(load_script(&renamed).unwrap().meta.name, "harvest");
    }

    #[test]
    fn ron_and_tomte_files_list_together() {
        let store = temp_store("mixed");
        let ron = store.save_new(&sample("legacy")).unwrap();
        let tomte = store.save_new_script("modern", "click left\n").unwrap();
        assert_eq!(store.list().unwrap(), {
            let mut want = vec![ron.clone(), tomte.clone()];
            want.sort();
            want
        });
        // Unrelated files are ignored.
        std::fs::write(store.dir().join("notes.txt"), "hi").unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn legacy_ron_loads_as_script_with_collapsed_clicks() {
        let store = temp_store("legacy-as-script");
        let mut file = sample("old timer");
        file.events = vec![
            MacroEvent {
                delay_us: 0,
                kind: EventKind::ButtonPress(MouseButton::Left),
            },
            MacroEvent {
                delay_us: 80_000,
                kind: EventKind::ButtonRelease(MouseButton::Left),
            },
        ];
        let path = store.save_new(&file).unwrap();
        let loaded = load_script(&path).unwrap();
        assert_eq!(loaded.meta.name, "old timer"); // meta.name wins over stem
        assert_eq!(
            loaded.body[0].instr,
            crate::script::Instr::Click {
                button: MouseButton::Left,
                at: None,
                double: false
            }
        );
    }

    #[test]
    fn malformed_tomte_errors_with_line_number() {
        let store = temp_store("bad-tomte");
        std::fs::create_dir_all(store.dir()).unwrap();
        let path = store.dir().join("broken.tomte");
        std::fs::write(&path, "click left\nclik right\n").unwrap();
        let err = load_script(&path).unwrap_err();
        assert!(
            err.to_string().contains("line 2"),
            "want line number in: {err}"
        );
    }

    #[test]
    fn timestamp_looks_like_rfc3339() {
        let ts = now_utc_rfc3339();
        // e.g. 2026-07-02T15:04:05Z
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
        assert!(ts.ends_with('Z'));
        assert!(ts.starts_with("20"));
    }
}
