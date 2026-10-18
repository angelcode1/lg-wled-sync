//! Stage 7: one native and one one-shot 320x180 JPEG of a known-clear
//! stationary owner-controlled scene. The installation wrapper separately
//! maintains and verifies Stage 1 receipts and removes all transient files.
use crate::capture::drm_routing::PlaybackClass;
use crate::capture::{luna_client, media};
use crate::ls2_access;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ONESHOT_URI: &str = "luna://com.webos.service.capture/executeOneShot";

fn output_dir() -> Result<PathBuf> {
    let supplied = std::env::var_os("C5_STAGE7_OUTPUT_DIR")
        .context("C5_STAGE7_REFUSE_OUTPUT_DIR_MISSING")?;
    let dir = PathBuf::from(supplied);
    let base = dir
        .parent()
        .context("C5_STAGE7_REFUSE_OUTPUT_DIR_NOT_NESTED")?;
    let name = base
        .file_name()
        .and_then(|part| part.to_str())
        .context("C5_STAGE7_REFUSE_STAGE_NAME")?;
    // Exact owner-created scratch namespace and nested private frames dir.
    // No arbitrary absolute output path, parent traversal or symlink.
    if !base.starts_with("/tmp")
        || base.parent() != Some(Path::new("/tmp"))
        || !name.starts_with("c5-ls2-stage7-")
        || !name
            .trim_start_matches("c5-ls2-stage7-")
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-')
        || name.len() < "c5-ls2-stage7-123-456".len()
        || dir.file_name().and_then(|part| part.to_str()) != Some("frames")
    {
        bail!("C5_STAGE7_REFUSE_UNEXPECTED_OUTPUT_LOCATION");
    }
    for target in [base, dir.as_path()] {
        let metadata = fs::symlink_metadata(target)?;
        if !metadata.file_type().is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            bail!("C5_STAGE7_REFUSE_UNOWNED_OR_NONPRIVATE_DIRECTORY");
        }
    }
    for leaf in ["native.jpg", "oneshot.jpg"] {
        let path = dir.join(leaf);
        if path.exists() || path.is_symlink() {
            bail!("C5_STAGE7_REFUSE_OUTPUT_ALREADY_EXISTS");
        }
    }
    Ok(dir)
}

async fn require_clear() -> Result<()> {
    if media::query().await != PlaybackClass::Clear {
        bail!("C5_STAGE7_REFUSE_NOT_CLEAR_NO_FRAME_REQUESTED");
    }
    Ok(())
}

pub async fn run(acknowledge: bool) -> Result<()> {
    if !acknowledge {
        bail!("C5_STAGE7_REFUSE_NEED_EXPLICIT_ACK");
    }
    let dir = output_dir()?;
    println!("C5_STAGE7_PREFLIGHT known_clear_required=true frames_max=2");
    // Independent stable classification: no fallback for HDMI, protected or
    // unknown. Native helper repeats it immediately before execute.
    for _ in 0..3 {
        require_clear().await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let native = dir.join("native.jpg");
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        ls2_access::run_stage7_native_clear_frame(runtime, native)
    })
    .await
    .context("C5_STAGE7_NATIVE_JOIN_FAILED")??;

    // The previous native LS2 handle was destroyed and client unregistered.
    // A changed, unavailable or ambiguous media classification aborts.
    require_clear().await?;
    let oneshot = dir.join("oneshot.jpg");
    let request = json!({
        "path": oneshot.to_str().context("C5_STAGE7_NON_UTF8_PATH")?,
        "method": "BLENDED",
        "width": 320,
        "height": 180,
        "format": "JPEG",
        "jpegQuality": 75
    });
    let started = Instant::now();
    let response = luna_client::request(
        ONESHOT_URI,
        &request.to_string(),
        Duration::from_millis(2300),
        8192,
    )
    .await
    .context("C5_STAGE7_ONESHOT_NO_REPLY_OR_TIMEOUT")?;
    let elapsed_ms = started.elapsed().as_millis();
    if response["returnValue"].as_bool() != Some(true) {
        bail!("C5_STAGE7_ONESHOT_REJECTED");
    }
    for (key, expected) in [("capturedWidth", 320), ("capturedHeight", 180)] {
        if let Some(value) = response.get(key) {
            if value.as_u64() != Some(expected) {
                bail!("C5_STAGE7_ONESHOT_DIMENSION_MISMATCH");
            }
        }
    }
    let bytes = ls2_access::verify_private_jpeg(&oneshot)?;
    if let Some(value) = response.get("writtenBytes") {
        if value.as_u64() != Some(bytes as u64) {
            bail!("C5_STAGE7_ONESHOT_LENGTH_MISMATCH");
        }
    }
    println!(
        "C5_STAGE7_ONESHOT_FRAME width=320 height=180 bytes={bytes} request_ms={elapsed_ms} staged_private=true quality75_requested=true quality75_verified=false"
    );
    // The wrapper transfers the two files over the owner's authenticated
    // local SSH session only if everything, including Stage 1 restoration,
    // succeeds, and then deletes them from both sides.
    println!("C5_STAGE7_PAIR_READY native_frames=1 oneshot_frames=1 no_protected_fallback=true");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn requires_explicit_ack_before_any_bus_call() {
        assert!(run(false).await.is_err());
    }
    #[test]
    fn refuses_unscoped_output_location() {
        std::env::remove_var("C5_STAGE7_OUTPUT_DIR");
        assert!(output_dir().is_err());
    }
}
