use std::ffi::OsStr;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;

const BIN_DIRNAME: &str = "bin";
const PACKAGE_METADATA_FILENAME: &str = "codex-package.json";
const PATH_DIRNAME: &str = "codex-path";
const RELEASES_DIRNAME: &str = "releases";
const RESOURCES_DIRNAME: &str = "codex-resources";
const STANDALONE_PACKAGES_DIRNAME: &str = "standalone";
const ZSH_DIRNAME: &str = "zsh";
static INSTALL_CONTEXT: OnceLock<InstallContext> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandalonePlatform {
    Unix,
    Windows,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexPackageLayout {
    /// The package root that contains the metadata file and layout directories.
    pub package_dir: AbsolutePathBuf,
    /// Directory containing the Codex entrypoint executable.
    pub bin_dir: AbsolutePathBuf,
    /// Directory containing managed helper binaries and data files, when present.
    pub resources_dir: Option<AbsolutePathBuf>,
    /// Folder that should be prepended to the PATH, when present.
    pub path_dir: Option<AbsolutePathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallContext {
    pub method: InstallMethod,
    pub package_layout: Option<CodexPackageLayout>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageMetadata {
    entrypoint: Option<PathBuf>,
    managed_codex: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallMethod {
    Standalone {
        /// The managed standalone release directory. Legacy installs use paths
        /// such as
        /// `~/.codex/packages/standalone/releases/0.111.0-x86_64-unknown-linux-musl`.
        /// Package-layout installs use the package root that contains `bin/`,
        /// `codex-resources/`, and `codex-path/`.
        release_dir: AbsolutePathBuf,
        /// The bundled resource directory for managed dependencies.
        resources_dir: Option<AbsolutePathBuf>,
        /// The platform of the standalone release, either `Unix` or `Windows`.
        platform: StandalonePlatform,
    },
    /// A Codex binary launched through the npm-managed `codex.js` shim.
    Npm,
    /// A Codex binary launched through the bun-managed `codex.js` shim.
    Bun,
    /// A Codex binary launched through the pnpm-managed `codex.js` shim.
    Pnpm,
    /// A Codex binary that appears to come from a Homebrew install prefix.
    Brew,
    /// Any other execution environment.
    ///
    /// This commonly covers `cargo run`, app-bundled Codex binaries, custom
    /// internal launchers, and tests that execute Codex from an arbitrary path.
    Other,
}

impl InstallContext {
    pub fn from_exe(
        is_macos: bool,
        current_exe: Option<&Path>,
        method_override: Option<InstallMethod>,
    ) -> Self {
        let codex_home = codex_utils_home_dir::find_codex_home().ok();
        Self::from_exe_with_codex_home(
            is_macos,
            current_exe,
            method_override,
            codex_home.as_deref(),
        )
    }

    fn from_exe_with_codex_home(
        is_macos: bool,
        current_exe: Option<&Path>,
        method_override: Option<InstallMethod>,
        codex_home: Option<&Path>,
    ) -> Self {
        let package_layout = current_exe.and_then(CodexPackageLayout::from_exe);
        let method = if let Some(method) = method_override {
            method
        } else if let Some(exe_path) = current_exe {
            install_method_from_exe(exe_path, codex_home, package_layout.as_ref(), is_macos)
        } else {
            InstallMethod::Other
        };

        Self {
            method,
            package_layout,
        }
    }

    pub fn current() -> &'static Self {
        INSTALL_CONTEXT.get_or_init(|| {
            let current_exe = std::env::current_exe().ok();
            // The npm/bun/pnpm wrapper env markers describe a Codex install; an
            // Open Interpreter binary inheriting them from the environment
            // (for example through a long-lived tmux server) must not adopt
            // a Codex install identity.
            let is_codex =
                codex_product_info::Product::current() == codex_product_info::Product::Codex;
            let method_override = if !is_codex {
                None
            } else if std::env::var_os("CODEX_MANAGED_BY_PNPM").is_some() {
                Some(InstallMethod::Pnpm)
            } else if std::env::var_os("CODEX_MANAGED_BY_NPM").is_some() {
                Some(InstallMethod::Npm)
            } else if std::env::var_os("CODEX_MANAGED_BY_BUN").is_some() {
                Some(InstallMethod::Bun)
            } else {
                None
            };
            Self::from_exe(
                cfg!(target_os = "macos"),
                current_exe.as_deref(),
                method_override,
            )
        })
    }

    pub fn rg_command(&self) -> PathBuf {
        if let Some(package_layout) = &self.package_layout
            && let Some(path_dir) = &package_layout.path_dir
        {
            let bundled_rg = path_dir.join(default_rg_command());
            if bundled_rg.is_file() {
                return bundled_rg.into_path_buf();
            }
        }

        if let InstallMethod::Standalone {
            resources_dir: Some(resources_dir),
            ..
        } = &self.method
        {
            let bundled_rg = resources_dir.join(default_rg_command());
            if bundled_rg.is_file() {
                return bundled_rg.into_path_buf();
            }
        }

        default_rg_command()
    }

    pub fn bundled_resource(&self, file_name: impl AsRef<Path>) -> Option<AbsolutePathBuf> {
        if let Some(package_layout) = &self.package_layout
            && let Some(resources_dir) = &package_layout.resources_dir
        {
            let resource = resources_dir.join(file_name.as_ref());
            if resource.is_file() {
                return Some(resource);
            }
        }

        if let InstallMethod::Standalone {
            resources_dir: Some(resources_dir),
            ..
        } = &self.method
        {
            let resource = resources_dir.join(file_name);
            if resource.is_file() {
                return Some(resource);
            }
        }

        None
    }

    pub fn bundled_zsh_path(&self) -> Option<AbsolutePathBuf> {
        if cfg!(windows) {
            None
        } else {
            self.bundled_resource(zsh_resource_path())
        }
    }

    pub fn bundled_zsh_bin_dir(&self) -> Option<AbsolutePathBuf> {
        self.bundled_zsh_path()?.parent()
    }
}

impl CodexPackageLayout {
    fn from_exe(exe_path: &Path) -> Option<Self> {
        let canonical_exe = canonical_absolute_path(exe_path)?;
        let exe_dir = canonical_exe.parent()?;
        match exe_dir.file_name() {
            Some(name) if name == OsStr::new(BIN_DIRNAME) => Self::from_package_bin_dir(exe_dir),
            Some(_) | None => None,
        }
    }

    fn from_package_bin_dir(bin_dir: AbsolutePathBuf) -> Option<Self> {
        let package_dir = bin_dir.parent()?;
        if !package_dir.join(PACKAGE_METADATA_FILENAME).is_file() {
            return None;
        }

        Some(Self {
            resources_dir: existing_dir(package_dir.join(RESOURCES_DIRNAME)),
            path_dir: existing_dir(package_dir.join(PATH_DIRNAME)),
            package_dir,
            bin_dir,
        })
    }
}

/// Resolve the managed Codex executable declared by a package-layout install.
pub fn managed_codex_bin_from_package_dir(package_dir: &Path) -> Option<AbsolutePathBuf> {
    let package_dir = canonical_absolute_path(package_dir)?;
    let metadata = read_package_metadata(&package_dir)?;
    let managed_codex = metadata
        .managed_codex
        .as_deref()
        .or(metadata.entrypoint.as_deref())?;
    package_relative_file(&package_dir, managed_codex)
}

fn install_method_from_exe(
    exe_path: &Path,
    codex_home: Option<&Path>,
    package_layout: Option<&CodexPackageLayout>,
    is_macos: bool,
) -> InstallMethod {
    if let Some(standalone_method) = standalone_install_method(exe_path, codex_home, package_layout)
    {
        return standalone_method;
    }

    if is_macos && (exe_path.starts_with("/opt/homebrew") || exe_path.starts_with("/usr/local")) {
        InstallMethod::Brew
    } else {
        InstallMethod::Other
    }
}

fn standalone_install_method(
    exe_path: &Path,
    codex_home: Option<&Path>,
    package_layout: Option<&CodexPackageLayout>,
) -> Option<InstallMethod> {
    let canonical_codex_home = canonical_absolute_path(codex_home?)?;
    let release_dir = if let Some(package_layout) = package_layout {
        package_layout.package_dir.clone()
    } else {
        canonical_absolute_path(exe_path)?.parent()?
    };
    let releases_root = canonical_codex_home
        .join("packages")
        .join(STANDALONE_PACKAGES_DIRNAME)
        .join(RELEASES_DIRNAME);
    if !release_dir.starts_with(releases_root.as_path()) {
        return None;
    }

    let resources_dir = release_dir.join(RESOURCES_DIRNAME);
    Some(InstallMethod::Standalone {
        release_dir,
        resources_dir: resources_dir.is_dir().then_some(resources_dir),
        platform: standalone_platform(),
    })
}

fn canonical_absolute_path(path: &Path) -> Option<AbsolutePathBuf> {
    let canonical_path = std::fs::canonicalize(path).ok()?;
    AbsolutePathBuf::from_absolute_path(canonical_path).ok()
}

fn standalone_platform() -> StandalonePlatform {
    if cfg!(windows) {
        StandalonePlatform::Windows
    } else {
        StandalonePlatform::Unix
    }
}

fn existing_dir(path: AbsolutePathBuf) -> Option<AbsolutePathBuf> {
    path.is_dir().then_some(path)
}

fn read_package_metadata(package_dir: &Path) -> Option<PackageMetadata> {
    let metadata = std::fs::read_to_string(package_dir.join(PACKAGE_METADATA_FILENAME)).ok()?;
    serde_json::from_str(&metadata).ok()
}

fn package_relative_file(
    package_dir: &AbsolutePathBuf,
    relative_path: &Path,
) -> Option<AbsolutePathBuf> {
    if !relative_path.is_relative() {
        return None;
    }
    if relative_path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }

    let path = package_dir.join(relative_path);
    path.is_file().then_some(path)
}

fn default_rg_command() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("rg.exe")
    } else {
        PathBuf::from("rg")
    }
}

fn zsh_resource_path() -> PathBuf {
    PathBuf::from(ZSH_DIRNAME).join(BIN_DIRNAME).join("zsh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs;

    const TEST_RESOURCE_NAME: &str = "codex-test-helper";

    #[test]
    fn detects_standalone_install_from_release_layout() -> std::io::Result<()> {
        let codex_home = tempfile::tempdir()?;
        let release_dir = codex_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        let resources_dir = release_dir.join(RESOURCES_DIRNAME);
        fs::create_dir_all(&resources_dir)?;
        let exe_path = release_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(default_rg_command()), "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        let canonical_release_dir =
            AbsolutePathBuf::from_absolute_path(release_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ Some(codex_home.path()),
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Standalone {
                    release_dir: canonical_release_dir,
                    resources_dir: Some(canonical_resources_dir.clone()),
                    platform: standalone_platform(),
                },
                package_layout: None,
            }
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        Ok(())
    }

    #[test]
    fn standalone_rg_falls_back_when_resources_are_missing() -> std::io::Result<()> {
        let codex_home = tempfile::tempdir()?;
        let release_dir = codex_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        fs::create_dir_all(&release_dir)?;
        let exe_path = release_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ Some(codex_home.path()),
        );
        assert_eq!(context.rg_command(), default_rg_command());
        Ok(())
    }

    #[test]
    fn detects_package_layout_independently_from_install_method() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let resources_dir = package_dir.path().join(RESOURCES_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&resources_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        if !cfg!(windows) {
            let zsh_path = resources_dir.join(zsh_resource_path());
            fs::create_dir_all(zsh_path.parent().expect("zsh path should have parent"))?;
            fs::write(&zsh_path, "")?;
        }
        let canonical_package_dir =
            AbsolutePathBuf::from_absolute_path(package_dir.path().canonicalize()?)?;
        let canonical_bin_dir = AbsolutePathBuf::from_absolute_path(bin_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;
        let package_layout = CodexPackageLayout {
            package_dir: canonical_package_dir,
            bin_dir: canonical_bin_dir,
            resources_dir: Some(canonical_resources_dir.clone()),
            path_dir: Some(canonical_path_dir.clone()),
        };

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ None,
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Other,
                package_layout: Some(package_layout),
            }
        );
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        if cfg!(windows) {
            assert_eq!(context.bundled_zsh_path(), None);
            assert_eq!(context.bundled_zsh_bin_dir(), None);
        } else {
            assert_eq!(
                context.bundled_zsh_path(),
                Some(canonical_resources_dir.join(zsh_resource_path()))
            );
            assert_eq!(
                context.bundled_zsh_bin_dir(),
                Some(canonical_resources_dir.join(ZSH_DIRNAME).join(BIN_DIRNAME))
            );
        }
        Ok(())
    }

    #[test]
    fn standalone_package_layout_keeps_standalone_install_method() -> std::io::Result<()> {
        let codex_home = tempfile::tempdir()?;
        let package_dir = codex_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        let bin_dir = package_dir.join(BIN_DIRNAME);
        let resources_dir = package_dir.join(RESOURCES_DIRNAME);
        let path_dir = package_dir.join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&resources_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        let canonical_package_dir =
            AbsolutePathBuf::from_absolute_path(package_dir.canonicalize()?)?;
        let canonical_bin_dir = AbsolutePathBuf::from_absolute_path(bin_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ Some(codex_home.path()),
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Standalone {
                    release_dir: canonical_package_dir.clone(),
                    resources_dir: Some(canonical_resources_dir.clone()),
                    platform: standalone_platform(),
                },
                package_layout: Some(CodexPackageLayout {
                    package_dir: canonical_package_dir,
                    bin_dir: canonical_bin_dir,
                    resources_dir: Some(canonical_resources_dir.clone()),
                    path_dir: Some(canonical_path_dir.clone()),
                }),
            }
        );
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        Ok(())
    }

    #[test]
    fn package_metadata_resolves_managed_codex_path() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        let managed_codex = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&managed_codex, "")?;
        fs::write(
            package_dir.path().join(PACKAGE_METADATA_FILENAME),
            format!(
                "{{\"managedCodex\":\"bin/{}\"}}",
                if cfg!(windows) { "codex.exe" } else { "codex" }
            ),
        )?;

        assert_eq!(
            managed_codex_bin_from_package_dir(package_dir.path()),
            Some(AbsolutePathBuf::from_absolute_path(
                managed_codex.canonicalize()?
            )?)
        );
        Ok(())
    }

    #[test]
    fn package_metadata_falls_back_to_entrypoint_for_managed_codex_path() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        let entrypoint = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&entrypoint, "")?;
        fs::write(
            package_dir.path().join(PACKAGE_METADATA_FILENAME),
            format!(
                "{{\"entrypoint\":\"bin/{}\"}}",
                if cfg!(windows) { "codex.exe" } else { "codex" }
            ),
        )?;

        assert_eq!(
            managed_codex_bin_from_package_dir(package_dir.path()),
            Some(AbsolutePathBuf::from_absolute_path(
                entrypoint.canonicalize()?
            )?)
        );
        Ok(())
    }

    #[test]
    fn package_metadata_rejects_managed_codex_paths_outside_package() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        fs::write(
            package_dir.path().join(PACKAGE_METADATA_FILENAME),
            "{\"managedCodex\":\"../codex\"}",
        )?;

        assert_eq!(managed_codex_bin_from_package_dir(package_dir.path()), None);
        Ok(())
    }

    #[test]
    fn npm_managed_package_keeps_package_layout() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;

        let context = InstallContext::from_exe(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ Some(InstallMethod::Npm),
        );
        assert_eq!(context.method, InstallMethod::Npm);
        assert!(context.package_layout.is_some());
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        Ok(())
    }

    #[test]
    fn standalone_package_rg_falls_back_when_codex_path_is_missing() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ None,
        );
        assert_eq!(context.rg_command(), default_rg_command());
        Ok(())
    }

    #[test]
    fn bundled_file_lookups_ignore_directories() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let resources_dir = package_dir.path().join(RESOURCES_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(resources_dir.join(TEST_RESOURCE_NAME))?;
        fs::create_dir_all(path_dir.join(default_rg_command()))?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*method_override*/ None,
            /*codex_home*/ None,
        );
        assert_eq!(context.rg_command(), default_rg_command());
        assert_eq!(context.bundled_resource(TEST_RESOURCE_NAME), None);
        Ok(())
    }

    #[test]
    fn package_manager_method_overrides_take_precedence() {
        let pnpm_context = InstallContext::from_exe(
            /*is_macos*/ false,
            /*current_exe*/ Some(Path::new("/tmp/codex")),
            /*method_override*/ Some(InstallMethod::Pnpm),
        );
        assert_eq!(
            pnpm_context,
            InstallContext {
                method: InstallMethod::Pnpm,
                package_layout: None,
            }
        );

        let npm_context = InstallContext::from_exe(
            /*is_macos*/ false,
            /*current_exe*/ Some(Path::new("/tmp/codex")),
            /*method_override*/ Some(InstallMethod::Npm),
        );
        assert_eq!(
            npm_context,
            InstallContext {
                method: InstallMethod::Npm,
                package_layout: None,
            }
        );

        let bun_context = InstallContext::from_exe(
            /*is_macos*/ false,
            /*current_exe*/ Some(Path::new("/tmp/codex")),
            /*method_override*/ Some(InstallMethod::Bun),
        );
        assert_eq!(
            bun_context,
            InstallContext {
                method: InstallMethod::Bun,
                package_layout: None,
            }
        );
    }

    #[test]
    fn brew_is_detected_on_macos_prefixes() {
        let context = InstallContext::from_exe_with_codex_home(
            /*is_macos*/ true,
            /*current_exe*/ Some(Path::new("/opt/homebrew/bin/codex")),
            /*method_override*/ None,
            /*codex_home*/ None,
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Brew,
                package_layout: None,
            }
        );
    }
}
