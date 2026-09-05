//! Exercise the real helper through installed paths and bounded process I/O.

use std::fs;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use codex_install_context::InstallContext;
use codex_realtime_webrtc::Message;
use codex_realtime_webrtc::VoiceHost;
use codex_realtime_webrtc::decode_frame;
use codex_realtime_webrtc::encode_frame;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(/*secs*/ 10);

fn spawn() -> Result<Child> {
    Ok(Command::new(cargo_bin("codex-voice-host")?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?)
}

async fn build_commit() -> Result<String> {
    let output = timeout(
        DEADLINE,
        Command::new(cargo_bin("codex-voice-host")?)
            .arg("--build-commit")
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    assert!(output.status.success());
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

async fn handshake(child: &mut Child) -> Result<()> {
    child
        .stdin
        .as_mut()
        .context("stdin")?
        .write_all(&encode_frame(&Message::Hello {
            protocol: 1,
            build_commit: build_commit().await?,
        })?)
        .await?;
    let expected = encode_frame(&Message::Ready {})?;
    let mut reply = vec![0; expected.len()];
    timeout(
        DEADLINE,
        child
            .stdout
            .as_mut()
            .context("stdout")?
            .read_exact(&mut reply),
    )
    .await??;
    assert_eq!(reply, expected);
    Ok(())
}

#[tokio::test]
async fn closes_after_acknowledgement_and_on_parent_pipe_loss() -> Result<()> {
    for explicit_close in [true, false] {
        let mut child = spawn()?;
        handshake(&mut child).await?;
        if explicit_close {
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(&encode_frame(&Message::Close {})?)
                .await?;
        }
        drop(child.stdin.take());
        let output = timeout(DEADLINE, child.wait_with_output()).await??;
        assert!(output.status.success());
        assert_eq!(
            output.stdout,
            if explicit_close {
                encode_frame(&Message::Closed {})?
            } else {
                vec![]
            }
        );
        assert!(output.stderr.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn rejects_invalid_input_without_echoing_it() -> Result<()> {
    assert!(decode_frame(b"\0\0\0\x16{\"type\":\"close\",\"x\":0}").is_err());
    let mut invalid_json = 22_u32.to_be_bytes().to_vec();
    invalid_json.extend_from_slice(b"sensitive-invalid-json");
    for frame in [
        u32::MAX.to_be_bytes().to_vec(),
        vec![0, 0],
        invalid_json,
        encode_frame(&Message::Hello {
            protocol: 1,
            build_commit: "wrong-build".into(),
        })?,
        encode_frame(&Message::Hello {
            protocol: 99,
            build_commit: build_commit().await?,
        })?,
        encode_frame(&Message::Close {})?,
    ] {
        let mut child = spawn()?;
        child.stdin.take().unwrap().write_all(&frame).await?;
        let output = timeout(DEADLINE, child.wait_with_output()).await??;
        assert!(!output.status.success());
        assert_eq!((output.stdout, output.stderr), (vec![], vec![]));
    }
    Ok(())
}
#[tokio::test]
async fn installed_client_rejects_mixed_builds_and_missing_helper() -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("voice package ")
        .tempdir()?;
    let bin = directory.path().join("bin");
    let helper_dir = directory.path().join("codex-resources/voice/bin");
    fs::create_dir_all(&bin)?;
    fs::create_dir_all(&helper_dir)?;
    fs::write(directory.path().join("codex-package.json"), "{}")?;
    let app = bin.join(if cfg!(windows) { "codex.exe" } else { "codex" });
    fs::write(&app, [])?;
    let source = cargo_bin("codex-voice-host")?;
    let helper = helper_dir.join(source.file_name().unwrap());
    fs::copy(&source, &helper)?;
    let package = InstallContext::from_exe(
        /*is_macos*/ cfg!(target_os = "macos"),
        Some(&app),
        /*method_override*/ None,
    )
    .package_layout
    .unwrap();
    VoiceHost::connect(&package, &build_commit().await?)
        .await?
        .close()
        .await?;
    assert!(VoiceHost::connect(&package, "wrong-build").await.is_err());
    // The same executable elsewhere in the package must not become a fallback.
    fs::rename(&helper, bin.join(source.file_name().unwrap()))?;
    assert!(
        VoiceHost::connect(&package, &build_commit().await?)
            .await
            .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, &helper)?;
        assert!(
            VoiceHost::connect(&package, &build_commit().await?)
                .await
                .is_err()
        );
    }
    Ok(())
}

// Linux permits raw-byte filenames; macOS filesystems reject this name themselves.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn installed_client_accepts_non_utf8_package_path() -> Result<()> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir()?;
    let root = directory
        .path()
        .join(OsString::from_vec(b"voice-\xff".to_vec()));
    let bin = root.join("bin");
    let helper_dir = root.join("codex-resources/voice/bin");
    fs::create_dir_all(&bin)?;
    fs::create_dir_all(&helper_dir)?;
    fs::write(root.join("codex-package.json"), "{}")?;
    let app = bin.join("codex");
    fs::write(&app, [])?;
    let source = cargo_bin("codex-voice-host")?;
    fs::copy(&source, helper_dir.join(source.file_name().unwrap()))?;
    let package = InstallContext::from_exe(
        /*is_macos*/ cfg!(target_os = "macos"),
        Some(&app),
        /*method_override*/ None,
    )
    .package_layout
    .context("package layout")?;
    VoiceHost::connect(&package, &build_commit().await?)
        .await?
        .close()
        .await
}
