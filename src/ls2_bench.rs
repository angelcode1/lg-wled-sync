//! Opt-in Stage 5 diagnostic: 5 native handle-reuse JPEG frames versus
//! 5 LG executeOneShot JPEG frames, never run in the daemon.
//! Denies Unknown/Protected media, bounds outputs and reports total
//! client, helper-child and LG captureservice CPU separately.
use crate::capture::drm_routing::PlaybackClass;
use crate::capture::{luna_client, media};
use crate::ls2_access;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const N: u8 = 5;
const WIDTH: u16 = 320;
const HEIGHT: u16 = 180;
const ONESHOT_URI: &str = "luna://com.webos.service.capture/executeOneShot";
const CAPTURE_EXE: &str = "/usr/sbin/captureservice";

pub async fn run(acknowledge: bool) -> Result<()> {
    if !acknowledge {
        bail!("pass --acknowledge-stage5-clear-benchmark before 10 bounded captures");
    }
    println!("C5_STAGE5_PREFLIGHT stable_clear_checks=3 protected_unknown_refuse=true");
    for _ in 0..3 {
        require_clear().await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let clk = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if clk <= 0 {
        bail!("C5_STAGE5_REFUSE_CLK_TCK_UNAVAILABLE");
    }
    // Require unambiguous, anchored LG service identity. A missing or
    // restarting process must never be misreported as 0% CPU.
    let first_service = get_capture_service()?;
    println!(
        "C5_STAGE5_CPU_SOURCE pid={} start_ticks={} clock_hz={clk}",
        first_service.pid, first_service.start_time
    );

    let native_before = CpuSnapshot::sample()?;
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || ls2_access::run_five_native_clear_frames(runtime))
        .await
        .context("C5_STAGE5_NATIVE_THREAD_JOIN_FAILED")??;
    let native_after = CpuSnapshot::sample()?;
    print_cpu("native", &native_before, &native_after, clk)?;
    println!("C5_STAGE5_NATIVE_PHASE_COMPLETE frames=5");

    // Native client has unregistered and destroyed its handle. Never run
    // the two phases concurrently or retain any screenshot.
    let shots_before = CpuSnapshot::sample()?;
    execute_oneshot_five().await?;
    let shots_after = CpuSnapshot::sample()?;
    print_cpu("oneshot", &shots_before, &shots_after, clk)?;
    println!(
        "C5_STAGE5_BENCH_PASS native_frames=5 oneshot_frames=5 \
         format=JPEG dimensions=320x180 one_shot_quality_75_requested=true \
         one_shot_quality_verified=false no_frame_retained=true"
    );
    Ok(())
}

async fn require_clear() -> Result<()> {
    if media::query().await != PlaybackClass::Clear {
        bail!("C5_STAGE5_REFUSE_MEDIA_NOT_CLEAR no_frame_requested=true");
    }
    Ok(())
}

struct ScopedOutputDir {
    dir: PathBuf,
}
impl ScopedOutputDir {
    fn new() -> Result<Self> {
        let dir = PathBuf::from(format!("/tmp/lg-hue-sync-stage5-{}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .context("C5_STAGE5_REFUSE_PRIVATE_DIRECTORY")?;
        Ok(Self { dir })
    }
    fn filename(&self, index: u8) -> PathBuf {
        self.dir.join(format!("oneshot-{index:02}.jpg"))
    }
}
impl Drop for ScopedOutputDir {
    fn drop(&mut self) {
        // Delete only the known, bounded output names; no arbitrary glob
        // or deletion of directories outside our just-created path.
        for i in 1..=N {
            let _ = fs::remove_file(self.filename(i));
        }
        let _ = fs::remove_dir(&self.dir);
    }
}

async fn execute_oneshot_five() -> Result<()> {
    let temp = ScopedOutputDir::new()?;
    for index in 1..=N {
        require_clear().await?;
        let path = temp.filename(index);
        if path.exists() || path.is_symlink() {
            bail!("C5_STAGE5_REFUSE_EXISTING_ONESHOT_IMAGE");
        }
        let body = json!({
            "path": path.to_str().context("non-UTF8 capture path")?,
            "method": "BLENDED",
            "width": WIDTH,
            "height": HEIGHT,
            "format": "JPEG",
            "jpegQuality": 75
        });
        let start = Instant::now();
        let response = luna_client::request(
            ONESHOT_URI,
            &body.to_string(),
            Duration::from_millis(2300),
            8192,
        )
        .await
        .context("C5_STAGE5_ONESHOT_DENIED_OR_TIMEOUT")?;
        let request_ms = start.elapsed().as_millis();
        if response["returnValue"].as_bool() != Some(true) {
            bail!("C5_STAGE5_ONESHOT_RETURNED_FAILURE");
        }
        // Fail on contradictory dimension metadata when provided.
        for (key, expected) in [("capturedWidth", WIDTH), ("capturedHeight", HEIGHT)] {
            if let Some(value) = response.get(key) {
                if value.as_u64() != Some(u64::from(expected)) {
                    bail!("C5_STAGE5_ONESHOT_DIMENSION_REPLY_MISMATCH");
                }
            }
        }
        let bytes = ls2_access::verify_private_jpeg(&path)?;
        if let Some(length) = response.get("writtenBytes") {
            if length.as_u64() != Some(bytes as u64) {
                bail!("C5_STAGE5_ONESHOT_WRITTEN_BYTES_MISMATCH");
            }
        }
        println!(
            "C5_STAGE5_ONESHOT_FRAME index={index} width={WIDTH} height={HEIGHT} \
             bytes={bytes} request_ms={request_ms}"
        );
        fs::remove_file(&path).context("C5_STAGE5_ONESHOT_DELETE_FAILED")?;
    }
    println!("C5_STAGE5_ONESHOT_PASS frames=5 no_frame_retained=true");
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct ServiceCpu {
    pid: u32,
    start_time: u64,
    total_ticks: u64,
}

fn parse_service_stat(pid: u32, raw: &str) -> Result<ServiceCpu> {
    let (_, tail) = raw
        .rsplit_once(") ")
        .context("invalid proc stat process name")?;
    let fields: Vec<&str> = tail.split_whitespace().collect();
    if fields.len() < 20 {
        bail!("proc stat missing fields");
    }
    let user: u64 = fields[11].parse()?;
    let system: u64 = fields[12].parse()?;
    Ok(ServiceCpu {
        pid,
        start_time: fields[19].parse()?,
        total_ticks: user.checked_add(system).context("proc tick overflow")?,
    })
}

fn get_capture_service() -> Result<ServiceCpu> {
    let mut found = None;
    for entry in fs::read_dir("/proc").context("cannot read /proc")? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(pid) = filename.to_str().and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let base = entry.path();
        if fs::read_link(base.join("exe")).ok().as_deref() != Some(Path::new(CAPTURE_EXE)) {
            continue;
        }
        let raw = fs::read_to_string(base.join("stat"))?;
        let cpu = parse_service_stat(pid, &raw)?;
        if found.replace(cpu).is_some() {
            bail!("C5_STAGE5_REFUSE_MULTIPLE_CAPTURE_SERVICES");
        }
    }
    found.context("C5_STAGE5_REFUSE_CAPTURE_SERVICE_CPU_UNAVAILABLE")
}

fn rusage_microseconds(who: libc::c_int) -> Result<u128> {
    let mut rusage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    if unsafe { libc::getrusage(who, rusage.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("getrusage failed");
    }
    let rusage = unsafe { rusage.assume_init() };
    fn micros(time: libc::timeval) -> Result<u128> {
        let sec = u128::try_from(time.tv_sec)?;
        let usec = u128::try_from(time.tv_usec)?;
        sec.checked_mul(1_000_000)
            .and_then(|v| v.checked_add(usec))
            .context("invalid timeval")
    }
    micros(rusage.ru_utime)?
        .checked_add(micros(rusage.ru_stime)?)
        .context("rusage overflow")
}

struct CpuSnapshot {
    wall: Instant,
    rust_us: u128,
    child_us: u128,
    service: ServiceCpu,
}
impl CpuSnapshot {
    fn sample() -> Result<Self> {
        Ok(Self {
            wall: Instant::now(),
            rust_us: rusage_microseconds(libc::RUSAGE_SELF)?,
            child_us: rusage_microseconds(libc::RUSAGE_CHILDREN)?,
            service: get_capture_service()?,
        })
    }
}

fn print_cpu(
    phase: &str,
    start: &CpuSnapshot,
    end: &CpuSnapshot,
    clock_hz: libc::c_long,
) -> Result<()> {
    if start.service.pid != end.service.pid || start.service.start_time != end.service.start_time {
        bail!("C5_STAGE5_REFUSE_CAPTURE_SERVICE_RESTARTED");
    }
    let wall_ms = end.wall.duration_since(start.wall).as_millis().max(1);
    let rust_us = end
        .rust_us
        .checked_sub(start.rust_us)
        .context("Rust CPU counter reset")?;
    let child_us = end
        .child_us
        .checked_sub(start.child_us)
        .context("child CPU counter reset")?;
    let ticks = end
        .service
        .total_ticks
        .checked_sub(start.service.total_ticks)
        .context("LG captureservice tick reset")?;
    let lg_ms = ticks as f64 * 1000.0 / clock_hz as f64;
    // "CPU percent" means 100% = one fully occupied core, not
    // a share of all TV cores. Media classification time is included.
    let rust_ms = rust_us as f64 / 1000.0;
    let child_ms = child_us as f64 / 1000.0;
    let total_ms = rust_ms + child_ms + lg_ms;
    println!(
        "C5_STAGE5_CPU phase={phase} wall_ms={wall_ms} rust_cpu_ms={rust_ms:.2} \
         luna_children_cpu_ms={child_ms:.2} lg_service_cpu_ms={lg_ms:.2} \
         total_measured_cpu_ms={total_ms:.2} one_core_cpu_pct={:.1}",
        total_ms / wall_ms as f64 * 100.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_comm_with_embedded_parenthesis_and_starttime() {
        let fields = (3..=25)
            .map(|index| match index {
                14 => "14",
                15 => "17",
                22 => "3123",
                _ => "0",
            })
            .collect::<Vec<_>>()
            .join(" ");
        let stat = format!("123 (captureservice (helper)) {fields}");
        let parsed = parse_service_stat(123, &stat).unwrap();
        assert_eq!(parsed.total_ticks, 31);
        assert_eq!(parsed.start_time, 3123);
        assert_eq!(parsed.pid, 123);
        assert!(parse_service_stat(1, "bad").is_err());
    }
    #[tokio::test]
    async fn acknowledgement_required_before_any_device_access() {
        assert!(run(false).await.is_err());
    }
}
