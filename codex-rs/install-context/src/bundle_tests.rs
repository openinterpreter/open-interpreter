//! Ensure a provisioned CLI still discovers its outer package and install method.

use super::*;
use pretty_assertions::assert_eq;
use std::fs;

#[test]
fn bundle_executable_preserves_package_layout_and_install_method() -> std::io::Result<()> {
    let home = tempfile::tempdir()?;
    let package = home.path().join("packages/standalone/releases/test");
    let executable = package.join("CodexCLI.app/Contents/MacOS/codex");
    fs::create_dir_all(executable.parent().unwrap())?;
    fs::write(&executable, "")?;
    for directory in [BIN_DIRNAME, RESOURCES_DIRNAME, PATH_DIRNAME] {
        fs::create_dir_all(package.join(directory))?;
    }
    fs::write(
        package.join(PACKAGE_METADATA_FILENAME),
        r#"{"version":"1.2.3"}"#,
    )?;
    let package = canonical_absolute_path(&package).unwrap();
    let bin_dir = package.join(BIN_DIRNAME);
    let resources_dir = package.join(RESOURCES_DIRNAME);
    let path_dir = package.join(PATH_DIRNAME);
    let context = InstallContext::from_exe_with_codex_home(
        /*is_macos*/ true,
        /*current_exe*/ Some(&executable),
        /*method_override*/ None,
        /*codex_home*/ Some(home.path()),
    );
    assert_eq!(
        context,
        InstallContext {
            method: InstallMethod::Standalone {
                release_dir: package.clone(),
                resources_dir: Some(resources_dir.clone()),
                platform: standalone_platform(),
            },
            package_layout: Some(CodexPackageLayout {
                package_dir: package,
                bin_dir,
                resources_dir: Some(resources_dir),
                path_dir: Some(path_dir),
            }),
        }
    );
    Ok(())
}
