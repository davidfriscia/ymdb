//! Operating system specific details of the MAME integration.
//!
//! Every platform-dependent behavior of the `mame` module lives here, so the
//! other modules stay platform-neutral.

use std::env;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// File names under which MAME executables are commonly distributed.
#[cfg(windows)]
pub const EXECUTABLE_NAMES: &[&str] = &["mame.exe", "mame64.exe"];
#[cfg(not(windows))]
pub const EXECUTABLE_NAMES: &[&str] = &["mame"];

/// The MAME joystick providers (`-joystickprovider`) whose device ids YMDB
/// maps, the one `auto` selects first. Windows builds of MAME 0.289 offer
/// `winhybrid`, `dinput`, `xinput` and `sdljoy`; SDL builds offer `sdljoy`.
#[cfg(windows)]
pub const JOYSTICK_PROVIDERS: &[&str] = &["winhybrid", "dinput", "xinput", "sdljoy"];
#[cfg(not(windows))]
pub const JOYSTICK_PROVIDERS: &[&str] = &["sdljoy"];

/// Filter for the native file picker: a display name and file extensions.
/// `None` when executables have no conventional extension.
#[cfg(windows)]
pub const EXECUTABLE_FILE_FILTER: Option<(&str, &[&str])> = Some(("MAME executable", &["exe"]));
#[cfg(not(windows))]
pub const EXECUTABLE_FILE_FILTER: Option<(&str, &[&str])> = None;

/// Conventional installation directories. They are searched non-recursively.
pub fn conventional_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(windows)]
    {
        // `SystemDrive` is a drive letter such as `C:`; append a separator to
        // obtain the drive root instead of the drive's current directory.
        let mut system_root = env::var_os("SystemDrive").unwrap_or_else(|| "C:".into());
        system_root.push("\\");
        let system_root = PathBuf::from(system_root);
        dirs.push(system_root.join("mame"));
        // Chocolatey installs MAME here.
        dirs.push(system_root.join("tools").join("mame"));
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(program_files) = env::var_os(variable) {
                dirs.push(PathBuf::from(program_files).join("MAME"));
            }
        }
        if let Some(home) = home_dir() {
            dirs.push(home.join("mame"));
            // Scoop installs MAME here.
            dirs.push(home.join("scoop").join("apps").join("mame").join("current"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/Applications/mame"));
        if let Some(home) = home_dir() {
            dirs.push(home.join("mame"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/games"));
        dirs.push(PathBuf::from("/usr/local/games"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/snap/bin"));
        if let Some(home) = home_dir() {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join("mame"));
        }
    }

    dirs
}

fn home_dir() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Returns whether the file can be executed directly, without a shell.
///
/// On Windows only `.exe` files qualify: other launchable types such as `.bat`
/// or `.cmd` are run through `cmd.exe` and are therefore rejected.
#[cfg(windows)]
pub fn is_executable(path: &Path, _metadata: &Metadata) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

/// Returns whether any execute permission bit is set.
#[cfg(unix)]
pub fn is_executable(_path: &Path, metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(any(windows, unix)))]
pub fn is_executable(_path: &Path, _metadata: &Metadata) -> bool {
    true
}

/// Returns whether a process spawn error means the file is not a runnable program.
pub fn is_not_executable_error(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    // ERROR_BAD_EXE_FORMAT, ERROR_EXE_MACHINE_TYPE_MISMATCH, and
    // ERROR_FILE_CORRUPT (returned for truncated or malformed executables).
    #[cfg(windows)]
    const NOT_EXECUTABLE_CODES: &[i32] = &[193, 216, 1392];
    // ENOEXEC (same value on Linux and macOS).
    #[cfg(unix)]
    const NOT_EXECUTABLE_CODES: &[i32] = &[8];
    #[cfg(not(any(windows, unix)))]
    const NOT_EXECUTABLE_CODES: &[i32] = &[];

    error
        .raw_os_error()
        .is_some_and(|code| NOT_EXECUTABLE_CODES.contains(&code))
}

/// Prevents a console window from appearing when a console program is started
/// from the GUI application on Windows. No-op on other platforms.
pub fn configure_background_process(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Whether two paths name the same executable, following the platform's
/// comparison rules (see [`same_path`]).
pub fn same_executable(a: &Path, b: &Path) -> bool {
    same_path(a, b)
}

/// Whether two paths name the same file or directory, following the
/// platform's comparison rules: symbolic links and relative components are
/// resolved when the path exists, `/` and `\` are equivalent on Windows, and
/// names are compared case-insensitively on Windows and macOS, whose default
/// file systems ignore case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a == b || path_key(a) == path_key(b)
}

fn path_key(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    let text = if cfg!(windows) {
        text.replace('/', "\\")
    } else {
        text.into_owned()
    };
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Directories where MAME looks for `mame.ini`, in order: MAME's default
/// `inipath` for the platform, with `.` being the MAME directory (YMDB runs
/// MAME from there).
pub fn ini_search_dirs(mame_dir: &Path) -> Vec<PathBuf> {
    // MAME's `INI_PATH`: `.;ini;ini/presets` on Windows,
    // `$HOME/.mame;.;ini` on Linux, preceded by
    // `$HOME/Library/Application Support/mame` on macOS.
    #[cfg(windows)]
    let (home, tail) = (
        Vec::<PathBuf>::new(),
        vec![mame_dir.join("ini").join("presets")],
    );
    #[cfg(target_os = "macos")]
    let (home, tail) = (
        home_dir()
            .map(|home| {
                vec![
                    home.join("Library")
                        .join("Application Support")
                        .join("mame"),
                    home.join(".mame"),
                ]
            })
            .unwrap_or_default(),
        Vec::<PathBuf>::new(),
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let (home, tail) = (
        home_dir()
            .map(|home| vec![home.join(".mame")])
            .unwrap_or_default(),
        Vec::<PathBuf>::new(),
    );
    #[cfg(not(any(windows, unix)))]
    let (home, tail) = (Vec::<PathBuf>::new(), Vec::<PathBuf>::new());
    home.into_iter()
        .chain([mame_dir.to_path_buf(), mame_dir.join("ini")])
        .chain(tail)
        .collect()
}
