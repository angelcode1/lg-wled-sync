//! Opt-in LG-managed BLENDED screenshots, independent of /dev/video60.
//! A successful root-shell probe is not proof of daemon LS2 entitlement.
//! Never invoke this module before the DRM router has confirmed Clear.
use crate::capture::luna_client;
use anyhow::{anyhow, bail, Context, Result};
use flate2::read::ZlibDecoder;
use serde_json::json;
use std::fs;
use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const URI: &str = "luna://com.webos.service.capture/executeOneShot";
const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
const MAX_WIDTH: u32 = 640;
const MAX_HEIGHT: u32 = 360;
const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

pub struct BlendedFrame {
    pub pixels: Vec<u8>, // RGBA8; compatible with ColorPipeline::frame(..., false).
    pub width: u32,
    pub height: u32,
    pub finished: Instant,
}

/// File is scoped to an unpredictable private directory. The directory and
/// screenshot are removed even when a capture task is cancelled or times out.
struct TempImage {
    dir: PathBuf,
}
impl TempImage {
    fn new() -> Result<Self> {
        let num = NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed);
        let dir = PathBuf::from(format!(
            "/tmp/lg-hue-sync-blended-{}-{num}",
            std::process::id()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .context("create private BLENDED capture directory")?;
        Ok(Self { dir })
    }

    fn image_path(&self) -> PathBuf {
        self.dir.join("frame.png")
    }
}
impl Drop for TempImage {
    fn drop(&mut self) {
        // We own this newly created directory. No globbing or shared files.
        let _ = fs::remove_file(self.image_path());
        let _ = fs::remove_dir(&self.dir);
    }
}

/// One capture for an explicitly Clear source. No V4L2/device opens or
/// permission changes. No image is retained after the caller processes it.
/// Each request uses the existing bounded interactive Luna helper.
pub async fn capture_once(width: u32, height: u32) -> Result<BlendedFrame> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        bail!("BLENDED capture dimensions must be 1..=640 by 1..=360");
    }
    let temp = TempImage::new()?;
    let path = temp.image_path();
    let request = json!({
        "path": path.to_str().context("capture path is not UTF-8")?,
        "method": "BLENDED",
        "width": width,
        "height": height,
        "format": "png"
    });
    let reply = luna_client::request(URI, &request.to_string(), Duration::from_millis(1300), 8192)
        .await
        .ok_or_else(|| anyhow!("BLENDED Luna request failed or timed out"))?;
    if reply["returnValue"].as_bool() != Some(true) {
        bail!("LG capture service rejected BLENDED request");
    }
    if reply["capturedWidth"].as_u64() != Some(u64::from(width))
        || reply["capturedHeight"].as_u64() != Some(u64::from(height))
    {
        bail!("BLENDED returned unexpected capture dimensions");
    }
    let expected_len = reply["writtenBytes"]
        .as_u64()
        .context("BLENDED reply lacks writtenBytes")?;
    if expected_len == 0 || expected_len > MAX_PNG_BYTES as u64 {
        bail!("BLENDED returned invalid image length");
    }
    let metadata = fs::symlink_metadata(&path).context("BLENDED PNG file missing")?;
    if !metadata.file_type().is_file() || metadata.len() != expected_len {
        bail!("BLENDED image missing, not regular, or size mismatch");
    }
    let png = fs::read(&path).context("read BLENDED PNG")?;
    if png.len() as u64 != expected_len {
        bail!("BLENDED image changed during read");
    }
    let pixels = decode_png_rgb_or_rgba(&png, width, height)?;
    Ok(BlendedFrame {
        pixels,
        width,
        height,
        finished: Instant::now(),
    })
}

/// Accept only non-interlaced 8-bit RGB/RGBA PNGs with valid chunk CRCs.
/// Our proven C5 screenshots are RGB8; unsupported types fail closed.
fn decode_png_rgb_or_rgba(png: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    if !png.starts_with(PNG_MAGIC) || png.len() > MAX_PNG_BYTES {
        bail!("invalid PNG signature or oversized PNG");
    }
    let mut offset = PNG_MAGIC.len();
    let mut channels = 0usize;
    let mut saw_ihdr = false;
    let mut saw_iend = false;
    let mut compressed = Vec::new();
    while offset + 12 <= png.len() {
        let size = u32::from_be_bytes(png[offset..offset + 4].try_into()?) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|v| v.checked_add(size))
            .context("PNG chunk offset overflow")?;
        if end > png.len() {
            bail!("truncated PNG chunk");
        }
        let kind: &[u8; 4] = png[offset + 4..offset + 8].try_into()?;
        let chunk = &png[offset + 8..offset + 8 + size];
        let expected_crc = u32::from_be_bytes(png[end - 4..end].try_into()?);
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(chunk);
        if crc.finalize() != expected_crc {
            bail!("corrupt PNG chunk CRC");
        }
        match kind {
            b"IHDR" if !saw_ihdr && size == 13 && offset == PNG_MAGIC.len() => {
                let png_width = u32::from_be_bytes(chunk[0..4].try_into()?);
                let png_height = u32::from_be_bytes(chunk[4..8].try_into()?);
                if png_width != width
                    || png_height != height
                    || chunk[8] != 8
                    || chunk[10] != 0
                    || chunk[11] != 0
                    || chunk[12] != 0
                {
                    bail!("unsupported PNG geometry, bit depth, or interlace");
                }
                channels = match chunk[9] {
                    2 => 3,
                    6 => 4,
                    _ => bail!("only RGB8/RGBA8 BLENDED PNG is supported"),
                };
                saw_ihdr = true;
            }
            b"IDAT" if saw_ihdr && !saw_iend => {
                if compressed.len() + size > MAX_PNG_BYTES {
                    bail!("PNG compressed data exceeds limit");
                }
                compressed.extend_from_slice(chunk);
            }
            b"IEND" if saw_ihdr && size == 0 => {
                saw_iend = true;
                break;
            }
            // Unknown critical chunks are not safe to ignore.
            _ if kind[0] & 0x20 == 0 => bail!("unknown or out-of-order critical PNG chunk"),
            _ => {}
        }
        offset = end;
    }
    if !saw_iend || compressed.is_empty() || channels == 0 {
        bail!("incomplete PNG");
    }
    let stride = (width as usize)
        .checked_mul(channels)
        .context("PNG stride overflow")?;
    let expected = (stride + 1)
        .checked_mul(height as usize)
        .context("PNG allocation overflow")?;
    let mut raw = Vec::with_capacity(expected);
    ZlibDecoder::new(&compressed[..])
        .take(expected as u64 + 1)
        .read_to_end(&mut raw)
        .context("PNG zlib decode failed")?;
    if raw.len() != expected {
        bail!("PNG decompressed length mismatch");
    }
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let mut previous = vec![0u8; stride];
    let mut reconstructed = vec![0u8; stride];
    for y in 0..height as usize {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let filter = row[0];
        for x in 0..stride {
            let left = if x >= channels {
                reconstructed[x - channels]
            } else {
                0
            };
            let above = previous[x];
            let top_left = if x >= channels {
                previous[x - channels]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
                4 => {
                    let p = i32::from(left) + i32::from(above) - i32::from(top_left);
                    let a = (p - i32::from(left)).abs();
                    let b = (p - i32::from(above)).abs();
                    let c = (p - i32::from(top_left)).abs();
                    if a <= b && a <= c {
                        left
                    } else if b <= c {
                        above
                    } else {
                        top_left
                    }
                }
                _ => bail!("unsupported PNG filter"),
            };
            reconstructed[x] = row[x + 1].wrapping_add(predictor);
        }
        for x in 0..width as usize {
            let from = x * channels;
            let to = (y * width as usize + x) * 4;
            rgba[to..to + 3].copy_from_slice(&reconstructed[from..from + 3]);
            rgba[to + 3] = if channels == 4 {
                reconstructed[from + 3]
            } else {
                255
            };
        }
        previous.copy_from_slice(&reconstructed);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn chunk(dst: &mut Vec<u8>, typ: &[u8; 4], data: &[u8]) {
        dst.extend_from_slice(&(data.len() as u32).to_be_bytes());
        dst.extend_from_slice(typ);
        dst.extend_from_slice(data);
        let mut crc = crc32fast::Hasher::new();
        crc.update(typ);
        crc.update(data);
        dst.extend_from_slice(&crc.finalize().to_be_bytes());
    }

    fn image(mode: u8, filter: u8) -> Vec<u8> {
        let channels = if mode == 2 { 3 } else { 4 };
        let mut raw = vec![filter];
        raw.extend_from_slice(if channels == 3 {
            &[50, 100, 150][..]
        } else {
            &[50, 100, 150, 255][..]
        });
        let mut deflater = ZlibEncoder::new(Vec::new(), Compression::default());
        deflater.write_all(&raw).unwrap();
        let idat = deflater.finish().unwrap();
        let mut png = PNG_MAGIC.to_vec();
        chunk(
            &mut png,
            b"IHDR",
            &[0, 0, 0, 1, 0, 0, 0, 1, 8, mode, 0, 0, 0],
        );
        chunk(&mut png, b"IDAT", &idat);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn accepts_rgb_and_rgba() {
        assert_eq!(
            decode_png_rgb_or_rgba(&image(2, 0), 1, 1).unwrap(),
            [50, 100, 150, 255]
        );
        assert_eq!(
            decode_png_rgb_or_rgba(&image(6, 0), 1, 1).unwrap(),
            [50, 100, 150, 255]
        );
    }

    #[test]
    fn rejects_wrong_dimensions_crc_and_invalid_signature() {
        assert!(decode_png_rgb_or_rgba(&image(2, 0), 320, 180).is_err());
        let mut corrupt = image(2, 0);
        corrupt[24] = 1;
        assert!(decode_png_rgb_or_rgba(&corrupt, 1, 1).is_err());
        assert!(decode_png_rgb_or_rgba(b"not a PNG", 1, 1).is_err());
    }

    #[test]
    fn decoder_handles_png_sub_filter() {
        assert_eq!(
            decode_png_rgb_or_rgba(&image(2, 1), 1, 1).unwrap(),
            [50, 100, 150, 255]
        );
    }
}
