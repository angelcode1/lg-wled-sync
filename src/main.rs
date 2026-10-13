mod capture;
mod color;
mod config;
mod hue;
mod local_http;
mod ls2_probe;
mod nanoleaf;
mod output;
mod runtime;
mod setup;
mod tv_power;
mod video_mode;
mod web;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use capture::drm_routing::{DrmCaptureRouter, HistogramHealth, PlaybackClass, Source};
use capture::histogram::HistogramCapture;
use capture::{
    create_capture, detect_source_fps, CaptureRecovery, MockCapture, RecoveryAction, ScreenCapture,
    VtCapture,
};
use color::pipeline::{ColorField, ColorPipeline};
use color::{RgbColor, ZoneSampler};
use config::{Config, ConfigError};
use hue::{sync_entertainment_areas, HueDtlsClient, HueStreamPacketBuilder};
use nanoleaf::{NanoleafPerimeterSampler, NanoleafUdpStreamer};
use output::gate::OutputGate;
use output::perimeter::PerimeterSampler;
use output::wled::WledUdpStreamer;
use output::{ColorSource, HueOutput, LightFrame, LightOutput};
use runtime::{PendingCommands, PipelineState, RetryState};
use setup::{run_pair, run_pair_nanoleaf, run_sync_hue};
use web::{start_web_server, CalibrationPattern, LiveSettings, SharedState};

#[derive(Parser)]
#[command(name = "lg-hue-sync")]
#[command(version)]
#[command(about = "High-performance native screen capture and ambient lighting synchronizer for LG webOS (Hue & Nanoleaf 4D)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the live sync daemon
    Run {
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
    },
    /// Stream a test color pattern to Hue lights to verify DTLS connection
    TestPattern {
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
    },
    /// Stream a test color pattern to Nanoleaf 4D lightstrip on port 60222
    TestNanoleaf {
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
    },
    /// Run screen capture only and log sampled zone colors to console
    TestCapture {
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
        /// Report central raw NV12 statistics without saving frame pixels or sending light output
        #[arg(long)]
        raw_nv12_stats: bool,
    },
    /// Explicit, bounded native LG histogram probe; no config or light output
    TestHistogram {
        /// Confirm that this test temporarily initializes the LG VT histogram device
        #[arg(long)]
        acknowledge_native_capture: bool,
        /// Number of samples (2–20)
        #[arg(long, default_value_t = 8)]
        samples: u8,
        /// Delay between samples in milliseconds (100–2000)
        #[arg(long, default_value_t = 600)]
        interval_ms: u64,
    },
    /// Register/unregister a temporary private LS2 name without loading the VT capture driver
    TestLs2Registration {
        /// Explicitly acknowledge an active Luna bus registration attempt
        #[arg(long)]
        acknowledge_luna_registration: bool,
        /// Probe native VT's capture-client LSRegister name instead of Resource Manager
        #[arg(long)]
        capture_client: bool,
    },
    /// Discover and pair with a Philips Hue Bridge via pushlink button
    Pair {
        #[arg(short, long)]
        bridge: Option<String>,
        #[arg(short, long, default_value = "config.json")]
        output: PathBuf,
    },
    /// Re-sync entertainment area and 3D light coordinates from Philips Hue app
    SyncHue {
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
        #[arg(short, long)]
        area: Option<String>,
    },
    /// Pair with Nanoleaf 4D controller and save credentials to config
    PairNanoleaf {
        #[arg(short, long)]
        ip: Option<String>,
        #[arg(short, long, default_value = "config.json")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { config } => run_daemon(config).await,
        Commands::TestPattern { config } => run_test_pattern(config).await,
        Commands::TestNanoleaf { config } => run_test_nanoleaf(config).await,
        Commands::TestCapture {
            config,
            raw_nv12_stats,
        } => run_test_capture(config, raw_nv12_stats).await,
        Commands::TestHistogram {
            acknowledge_native_capture,
            samples,
            interval_ms,
        } => run_test_histogram(acknowledge_native_capture, samples, interval_ms).await,
        Commands::TestLs2Registration {
            acknowledge_luna_registration,
            capture_client,
        } => ls2_probe::run(acknowledge_luna_registration, capture_client),
        Commands::Pair { bridge, output } => run_pair(bridge, output).await,
        Commands::SyncHue { config, area } => run_sync_hue(config, area).await,
        Commands::PairNanoleaf { ip, config } => run_pair_nanoleaf(ip, config).await,
    }
}

fn reconnect_hue(config: &Config) -> Result<HueDtlsClient> {
    set_hue_stream_active(config, true)?;
    HueDtlsClient::connect(&config.bridge_ip, &config.username, &config.clientkey)
}

fn hue_configuration_id(config: &Config) -> Option<String> {
    config
        .entertainment_configuration_id
        .as_deref()
        .filter(|id| id.len() == 36)
        .map(ToOwned::to_owned)
}

fn set_hue_stream_active(config: &Config, active: bool) -> Result<()> {
    let configuration_id = hue_configuration_id(config)
        .ok_or_else(|| anyhow!("Hue V2 Entertainment configuration has not been selected"))?;
    hue::set_stream_active(
        &config.bridge_ip,
        &config.username,
        &configuration_id,
        config.hue_bridge_certificate_sha256.as_deref(),
        active,
    )
}

fn calibration_pattern(state: &SharedState) -> Option<CalibrationPattern> {
    *state.calibration_pattern.read().unwrap()
}

fn calibration_zone_colors(
    pattern: CalibrationPattern,
    zones: &[config::LightZone],
) -> Vec<(u8, RgbColor)> {
    zones
        .iter()
        .map(|zone| {
            (
                zone.channel_id,
                calibration_color(pattern, zone_center(zone)),
            )
        })
        .collect()
}

fn zone_center(zone: &config::LightZone) -> (f32, f32) {
    (
        (zone.x_min + zone.x_max) * 0.5,
        (zone.y_min + zone.y_max) * 0.5,
    )
}

fn calibration_perimeter_colors(pattern: CalibrationPattern, count: usize) -> Vec<RgbColor> {
    let active_segment = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| (duration.as_millis() / 150) as usize % count.max(1))
        .unwrap_or(0);
    (0..count)
        .map(|index| {
            if pattern == CalibrationPattern::Perimeter {
                return if index == active_segment {
                    RgbColor::new(255, 255, 255)
                } else {
                    RgbColor::new(0, 0, 0)
                };
            }
            let position = if count == 40 {
                if (16..=28).contains(&index) {
                    (0.5, 0.0)
                } else if (29..=35).contains(&index) {
                    (1.0, 0.5)
                } else if index <= 7 || index >= 36 {
                    (0.5, 1.0)
                } else {
                    (0.0, 0.5)
                }
            } else {
                (index as f32 / count.max(1) as f32, 0.5)
            };
            calibration_color(pattern, position)
        })
        .collect()
}

fn calibration_color(pattern: CalibrationPattern, (x, y): (f32, f32)) -> RgbColor {
    match pattern {
        CalibrationPattern::Red => RgbColor::new(255, 0, 0),
        CalibrationPattern::Green => RgbColor::new(0, 255, 0),
        CalibrationPattern::Blue => RgbColor::new(0, 0, 255),
        CalibrationPattern::White => RgbColor::new(255, 255, 255),
        CalibrationPattern::WarmWhite => RgbColor::new(255, 180, 107),
        CalibrationPattern::DaylightWhite => RgbColor::new(255, 255, 255),
        CalibrationPattern::Quadrants => {
            if y < 0.33 {
                RgbColor::new(255, 0, 0)
            } else if x > 0.66 {
                RgbColor::new(0, 255, 0)
            } else if y > 0.66 {
                RgbColor::new(0, 0, 255)
            } else {
                RgbColor::new(255, 255, 0)
            }
        }
        CalibrationPattern::Perimeter => RgbColor::new(255, 255, 255),
    }
}

/// A single published colour field is faned out to any enabled vendor.
/// Every adapter owns geometry and network protocol; this layer owns status,
/// throttling and reconnects, never reprocessing captured RGB values.
struct OutputFanout<'a> {
    field: &'a ColorField,
    config: &'a Config,
    settings: &'a LiveSettings,
    shared: &'a SharedState,
    wled: &'a mut [(WledUdpStreamer, PerimeterSampler, f32)],
    hue_transport: &'a mut Option<HueDtlsClient>,
    hue_packets: &'a mut Option<HueStreamPacketBuilder>,
    nanoleaf_transport: &'a mut Option<NanoleafUdpStreamer>,
    nanoleaf_layout: &'a Option<NanoleafPerimeterSampler>,
    hue_enabled: bool,
    nanoleaf_enabled: bool,
    hue_retry: &'a mut RetryState,
    nanoleaf_retry: &'a mut RetryState,
    gate: &'a mut OutputGate,
}

impl OutputFanout<'_> {
    fn publish(&mut self) -> bool {
        let mut any_sent = false;
        let calibration = calibration_pattern(self.shared);
        let source = self.field.source();
        let adaptive = self.config.adaptive_throttling;
        if self.field.scene_cut {
            self.gate.clear();
        }

        if self.hue_enabled {
            let ids: Vec<u8> = self
                .config
                .zones
                .iter()
                .map(|zone| zone.channel_id)
                .collect();
            let colors = if let Some(pattern) = calibration {
                calibration_zone_colors(pattern, &self.config.zones)
                    .into_iter()
                    .map(|(_, color)| color)
                    .collect::<Vec<_>>()
            } else {
                HueOutput::map_field(
                    self.field,
                    &self.config.zones,
                    self.settings.brightness_multiplier * self.settings.hue_output_brightness,
                )
            };
            *self.shared.live_hue_colors.write().unwrap() =
                ids.iter().copied().zip(colors.iter().copied()).collect();
            if self.gate.should_send("hue", &colors, adaptive) {
                if let (Some(dtls), Some(packets)) =
                    (self.hue_transport.as_mut(), self.hue_packets.as_mut())
                {
                    let mut adapter = HueOutput {
                        transport: dtls,
                        packets,
                        channel_ids: &ids,
                        use_xy: calibration.is_none() && self.settings.use_xy_gamut,
                    };
                    match adapter.send(&LightFrame {
                        colors: &colors,
                        source,
                    }) {
                        Ok(()) => {
                            self.gate.record("hue", &colors);
                            self.hue_retry.success();
                            self.shared.hue_connected.store(true, Ordering::Relaxed);
                            any_sent = true;
                        }
                        Err(error) => {
                            self.shared.hue_connected.store(false, Ordering::Relaxed);
                            warn!("{} output error: {error}", adapter.kind());
                            if self.hue_retry.ready() {
                                match reconnect_hue(self.config) {
                                    Ok(new_dtls) => {
                                        *dtls = new_dtls;
                                        self.hue_retry.success();
                                        self.shared.hue_connected.store(true, Ordering::Relaxed);
                                    }
                                    Err(err) => {
                                        self.hue_retry.failure();
                                        warn!("Hue reconnect failed: {err}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.nanoleaf_enabled {
            if let Some(layout) = self.nanoleaf_layout.as_ref() {
                let positions = layout.screen_positions();
                let colors = if let Some(pattern) = calibration {
                    calibration_perimeter_colors(pattern, positions.len())
                } else {
                    NanoleafUdpStreamer::map_field(
                        self.field,
                        &positions,
                        self.settings.brightness_multiplier
                            * self.settings.nanoleaf_output_brightness,
                    )
                };
                *self.shared.live_nanoleaf_colors.write().unwrap() = colors.clone();
                if self.gate.should_send("nanoleaf", &colors, adaptive) {
                    if let Some(transport) = self.nanoleaf_transport.as_mut() {
                        match transport.send(&LightFrame {
                            colors: &colors,
                            source,
                        }) {
                            Ok(()) => {
                                self.gate.record("nanoleaf", &colors);
                                self.nanoleaf_retry.success();
                                self.shared
                                    .nanoleaf_connected
                                    .store(true, Ordering::Relaxed);
                                any_sent = true;
                            }
                            Err(error) => {
                                self.shared
                                    .nanoleaf_connected
                                    .store(false, Ordering::Relaxed);
                                warn!("{} output error: {error}", transport.kind());
                                if self.nanoleaf_retry.ready() {
                                    if let Some(ncfg) = self.config.nanoleaf.as_ref() {
                                        match nanoleaf::enable_external_control(
                                            &ncfg.ip,
                                            &ncfg.auth_token,
                                        )
                                        .and_then(|port| {
                                            NanoleafUdpStreamer::new(
                                                &ncfg.ip,
                                                port,
                                                layout.panel_ids(),
                                            )
                                        }) {
                                            Ok(new_transport) => {
                                                *transport = new_transport;
                                                self.nanoleaf_retry.success();
                                            }
                                            Err(err) => {
                                                self.nanoleaf_retry.failure();
                                                warn!("Nanoleaf reconnect failed: {err}");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.config.wled_sync_enabled {
            let mut all_wled_ok = true;
            for (index, (transport, sampler, trim)) in self.wled.iter_mut().enumerate() {
                let colors = if let Some(pattern) = calibration {
                    calibration_perimeter_colors(pattern, transport.led_count())
                } else {
                    WledUdpStreamer::map_field(
                        self.field,
                        sampler.screen_positions(),
                        self.settings.brightness_multiplier * *trim,
                    )
                };
                let key = format!("wled-{index}");
                if !self.gate.should_send(&key, &colors, adaptive) {
                    continue;
                }
                match transport.send(&LightFrame {
                    colors: &colors,
                    source,
                }) {
                    Ok(()) => {
                        self.gate.record(&key, &colors);
                        any_sent = true;
                    }
                    Err(error) => {
                        all_wled_ok = false;
                        warn!("{} output {index} failed: {error}", transport.kind());
                    }
                }
            }
            // A successful UDP syscall is not a physical-controller ACK.
            self.shared
                .wled_last_send_ok
                .store(all_wled_ok, Ordering::Relaxed);
        }
        any_sent
    }
}

async fn run_daemon(config_path: PathBuf) -> Result<()> {
    let mut config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(ConfigError::Open { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            info!("No configuration found; starting dashboard setup mode.");
            Config::new_default("", "", "", "")
        }
        Err(error) => return Err(error.into()),
    };

    let hue_active =
        config.hue_enabled && !config.bridge_ip.is_empty() && !config.username.is_empty();
    let nanoleaf_active = config
        .nanoleaf
        .as_ref()
        .map(|n| n.enabled && !n.ip.is_empty() && !n.auth_token.is_empty())
        .unwrap_or(false);

    let mut observed_tv_power = if config.auto_tv_power {
        match tv_power::is_active().await {
            Ok(state) => state,
            Err(error) => {
                warn!("Could not read TV power state; starting normally and retrying: {error}");
                None
            }
        }
    } else {
        None
    };
    let pipeline_should_run = observed_tv_power != Some(false);
    if !pipeline_should_run && hue_active {
        let _ = set_hue_stream_active(&config, false);
    }

    if hue_active && hue_configuration_id(&config).is_none() {
        let bridge_identity = (
            config.bridge_ip.clone(),
            config.username.clone(),
            config.hue_bridge_certificate_sha256.clone(),
        );
        match sync_entertainment_areas(
            &config.bridge_ip,
            &config.username,
            config.entertainment_configuration_id.as_deref(),
            config.hue_bridge_certificate_sha256.as_deref(),
        ) {
            Ok(area) => {
                config.entertainment_area_id = area.configuration_id.clone();
                config.entertainment_configuration_id = Some(area.configuration_id);
                config.hue_bridge_certificate_sha256 = Some(area.certificate_sha256);
                config.zones = area.zones;
                if let Err(error) = Config::update(&config_path, None, |current| {
                    if (
                        current.bridge_ip.as_str(),
                        current.username.as_str(),
                        current.hue_bridge_certificate_sha256.as_ref(),
                    ) != (
                        bridge_identity.0.as_str(),
                        bridge_identity.1.as_str(),
                        bridge_identity.2.as_ref(),
                    ) {
                        anyhow::bail!("Hue Bridge configuration changed during sync; retry");
                    }
                    current.entertainment_area_id = config.entertainment_area_id.clone();
                    current.entertainment_configuration_id =
                        config.entertainment_configuration_id.clone();
                    current.hue_bridge_certificate_sha256 =
                        config.hue_bridge_certificate_sha256.clone();
                    current.zones = config.zones.clone();
                    Ok(())
                }) {
                    warn!(
                        "Resolved Hue V2 configuration but could not persist it: {}",
                        error
                    );
                }
            }
            Err(error) => warn!(
                "Hue V2 configuration discovery failed; using legacy packet header: {}",
                error
            ),
        }
    }

    info!(
        "Loaded configuration (Hue: {}, Nanoleaf: {})",
        if hue_active {
            format!("ACTIVE @ {}", config.bridge_ip)
        } else {
            "DISABLED".to_string()
        },
        if nanoleaf_active {
            format!("ACTIVE @ {}", config.nanoleaf.as_ref().unwrap().ip)
        } else {
            "DISABLED".to_string()
        }
    );

    // Setup graceful shutdown handler for SIGINT and SIGTERM
    let running = Arc::new(AtomicBool::new(true));
    let shutdown = CancellationToken::new();
    let signal_shutdown = shutdown.clone();
    let r = running.clone();
    let signal_task = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("Failed to register SIGTERM handler: {}", e);
                    None
                }
            };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received SIGINT (Ctrl+C). Stopping sync...");
                }
                _ = async {
                    if let Some(ref mut st) = sigterm {
                        st.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    info!("Received SIGTERM from systemd. Stopping sync gracefully...");
                }
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
            info!("Received shutdown signal. Stopping sync...");
        }
        r.store(false, Ordering::SeqCst);
        signal_shutdown.cancel();
    });

    // 1. Initialize Philips Hue DTLS client if active
    let mut hue_sync_enabled = config.hue_sync_enabled;
    let mut nanoleaf_sync_enabled = config.nanoleaf_sync_enabled;
    let mut hue_dtls = if hue_active && hue_sync_enabled && pipeline_should_run {
        match reconnect_hue(&config) {
            Ok(client) => Some(client),
            Err(error) => {
                warn!("Hue unavailable at start; keeping other outputs running: {error}");
                None
            }
        }
    } else {
        None
    };

    let mut hue_sampler = if hue_active {
        Some(ZoneSampler::new(
            config.zones.clone(),
            0.35,
            config.hdr_tone_mapping,
            config.letterbox_detection,
            config.saturation_boost,
            config.noise_gate_threshold,
        ))
    } else {
        None
    };
    if let Some(ref mut sampler) = hue_sampler {
        sampler.set_temporal_response(config.rise_smoothing_factor, config.fall_smoothing_factor);
        sampler.set_strict_blackout(config.strict_blackout);
        sampler.set_peak_weight(config.peak_weight);
        sampler.set_spatial_blend(config.spatial_blend);
        sampler.set_gamma(config.gamma);
        sampler.set_max_color_step(config.max_color_step);
    }

    let mut hue_packet_builder = if hue_active {
        Some(HueStreamPacketBuilder::new(hue_configuration_id(&config)))
    } else {
        None
    };

    // 2. Initialize Nanoleaf 4D UDP streamer if active
    let (mut nanoleaf_sampler, mut nanoleaf_streamer) = if nanoleaf_active {
        let n_cfg = config.nanoleaf.as_ref().unwrap();
        let sampler = NanoleafPerimeterSampler::new_aligned(
            n_cfg.segments,
            &n_cfg.panel_ids,
            config.hdr_tone_mapping,
            config.saturation_boost,
            config.noise_gate_threshold,
            config.brightness_multiplier * config.nanoleaf_output_brightness,
            n_cfg.alignment,
        );
        let streamer = if nanoleaf_sync_enabled && pipeline_should_run {
            match nanoleaf::enable_external_control(&n_cfg.ip, &n_cfg.auth_token)
                .and_then(|port| NanoleafUdpStreamer::new(&n_cfg.ip, port, sampler.panel_ids()))
            {
                Ok(streamer) => {
                    info!(
                        "Nanoleaf 4D streaming ready: {} segments",
                        streamer.panel_count()
                    );
                    Some(streamer)
                }
                Err(error) => {
                    warn!("Nanoleaf unavailable at start; keeping other outputs running: {error}");
                    None
                }
            }
        } else {
            None
        };
        (Some(sampler), streamer)
    } else {
        (None, None)
    };
    if let Some(ref mut sampler) = nanoleaf_sampler {
        sampler.set_temporal_response(config.rise_smoothing_factor, config.fall_smoothing_factor);
        sampler.set_strict_blackout(config.strict_blackout);
        sampler.set_peak_weight(config.peak_weight);
        sampler.set_spatial_blend(config.spatial_blend);
        sampler.set_gamma(config.gamma);
        sampler.set_max_color_step(config.max_color_step);
    }

    // Native WLED adapter: config owns physical LED geometry and output
    // transport. Hue/Nanoleaf credentials are entirely optional.
    let mut wled_outputs = Vec::new();
    if config.wled_sync_enabled {
        for device in config.wled_devices.iter().filter(|d| d.enabled) {
            match WledUdpStreamer::new(&device.ip, device.udp_port, device.led_count) {
                Ok(transport) => {
                    let sampler = PerimeterSampler::new(
                        device.led_count,
                        device.start_corner,
                        device.reverse_direction,
                        device.perimeter_offset,
                        config.saturation_boost,
                        config.noise_gate_threshold,
                        config.gamma,
                        config.strict_blackout,
                    );
                    info!(
                        "WLED output configured: {} LEDs -> {}",
                        device.led_count,
                        transport.target()
                    );
                    wled_outputs.push((transport, sampler, device.brightness_trim));
                }
                Err(e) => warn!("WLED output unavailable: {e}"),
            }
        }
    }

    // 3. Initialize capture
    // Start without native capture. The media router must grant a stable Clear state first.
    let mut capture: Box<dyn ScreenCapture> = Box::new(MockCapture::new(
        config.capture_width, config.capture_height,
    ));
    let mut color_pipeline = ColorPipeline::new(
        config.saturation_boost,
        config.noise_gate_threshold,
        config.letterbox_detection,
    );
    color_pipeline.tune(
        config.saturation_boost,
        config.noise_gate_threshold,
        config.gamma,
        config.peak_weight,
        config.rise_smoothing_factor,
        config.fall_smoothing_factor,
        config.strict_blackout,
        config.max_color_step,
        config.hdr_tone_mapping,
        config.letterbox_detection,
        config.spatial_blend,
    );
    let mut drm_router = DrmCaptureRouter::default();
    drm_router.set_mode(config.capture_mode);
    let mut histogram_capture: Option<HistogramCapture> = None;
    let mut using_histogram = false;
    let mut media_class = PlaybackClass::Unknown;
    // Long-lived Luna requests must not stall 30–60 FPS LED output. Poll
    // results in background tasks, without detaching uncontrolled processes.
    let mut media_query: Option<tokio::task::JoinHandle<PlaybackClass>> = None;
    let mut model_query: Option<tokio::task::JoinHandle<Option<String>>> = None;
    let mut tv_model: Option<String> = None;
    let mut last_model_check = tokio::time::Instant::now() - Duration::from_secs(31);
    let mut last_model_warning = tokio::time::Instant::now() - Duration::from_secs(61);
    let mut last_media_check = tokio::time::Instant::now() - Duration::from_secs(2);
    let mut last_histogram_attempt = tokio::time::Instant::now() - Duration::from_secs(10);
    let mut last_histogram_valid: Option<std::time::Instant> = None;
    let mut histogram_black_sent = false;

    // Determine target framerate (supports auto-matching source refresh rate 23.976..60.0 Hz)
    let detected_fps = if config.fps == 0 {
        detect_source_fps()
    } else {
        None
    };
    let initial_fps = if config.fps == 0 {
        detected_fps.unwrap_or(60.0)
    } else {
        config.fps as f64
    };
    let mut target_fps = initial_fps.clamp(20.0, 60.0);
    let mut frame_interval = Duration::from_secs_f64(1.0 / target_fps);

    info!(
        "Entering sync loop at {:.2} FPS ({:.2} ms cadence{}, mode: {}, HDR tone-mapping: {}, letterbox: {})...",
        target_fps,
        frame_interval.as_secs_f64() * 1000.0,
        if detected_fps.is_some() { " [source auto-matched]" } else { "" },
        if config.use_xy_gamut { "CIE 1931 xy (Gamut C)" } else { "sRGB" },
        config.hdr_tone_mapping,
        config.letterbox_detection
    );

    // Notify systemd that service is ready and streaming
    let status_desc = format!(
        "Streaming active ({:.1} FPS, {} Hue zones, {} Nanoleaf segments)",
        target_fps,
        if hue_active { config.zones.len() } else { 0 },
        nanoleaf_streamer
            .as_ref()
            .map(|s| s.panel_count())
            .unwrap_or(0)
    );
    let _ = sd_notify::notify(&[
        sd_notify::NotifyState::Ready,
        sd_notify::NotifyState::Status(&status_desc),
    ]);
    let mut watchdog = WatchdogHeartbeat::new();

    let initial_settings = LiveSettings {
        brightness_multiplier: config.brightness_multiplier,
        hue_output_brightness: config.hue_output_brightness,
        nanoleaf_output_brightness: config.nanoleaf_output_brightness,
        saturation_boost: config.saturation_boost,
        peak_weight: config.peak_weight,
        spatial_blend: config.spatial_blend,
        gamma: config.gamma,
        noise_gate_threshold: config.noise_gate_threshold,
        smoothing_factor: config.smoothing_factor,
        rise_smoothing_factor: config.rise_smoothing_factor,
        fall_smoothing_factor: config.fall_smoothing_factor,
        strict_blackout: config.strict_blackout,
        use_xy_gamut: config.use_xy_gamut,
        letterbox_detection: config.letterbox_detection,
        hdr_tone_mapping: config.hdr_tone_mapping,
        hue_sync_enabled,
        nanoleaf_sync_enabled,
        auto_tv_power: config.auto_tv_power,
        nanoleaf_alignment: config
            .nanoleaf
            .as_ref()
            .map(|nanoleaf| nanoleaf.alignment)
            .unwrap_or_default(),
        max_color_step: config.max_color_step,
    };

    let (command_tx, mut command_rx) = mpsc::channel(32);
    let shared_state = Arc::new(SharedState::new(
        initial_settings,
        config.bridge_ip.clone(),
        config
            .nanoleaf
            .as_ref()
            .map(|nanoleaf| nanoleaf.ip.clone())
            .unwrap_or_default(),
        format!("{}x{}", config.capture_width, config.capture_height),
        config.zones.clone(),
        command_tx,
    ));
    shared_state
        .is_syncing
        .store(pipeline_should_run, Ordering::Relaxed);
    shared_state.set_tv_power_state(observed_tv_power);
    shared_state.set_fps(target_fps as f32);

    shared_state
        .wled_count
        .store(wled_outputs.len() as u32, Ordering::Relaxed);
    shared_state
        .hue_connected
        .store(hue_dtls.is_some(), Ordering::Relaxed);
    shared_state
        .nanoleaf_connected
        .store(nanoleaf_streamer.is_some(), Ordering::Relaxed);
    shared_state
        .capture_hardware
        .store(capture.is_real_hardware(), Ordering::Relaxed);

    let web_shutdown = CancellationToken::new();
    let web_tasks = match start_web_server(
        8088,
        shared_state.clone(),
        config_path.clone(),
        web_shutdown.clone(),
    )
    .await
    {
        Ok(tasks) => Some(tasks),
        Err(error) => {
            warn!("Failed to start web control server on port 8088: {}", error);
            None
        }
    };

    let mut auto_tv_power = config.auto_tv_power;

    let mut output_gate = OutputGate::default();
    let mut light_update_count = 0u32;
    let mut light_update_window = tokio::time::Instant::now();
    let mut last_status_check = tokio::time::Instant::now();
    let mut capture_retry = RetryState::new();
    if !capture.is_real_hardware() {
        capture_retry.failure();
    }
    let mut capture_recovery = CaptureRecovery::default();
    let mut capture_faulted = false;
    let mut last_tv_power_poll = tokio::time::Instant::now() - Duration::from_secs(2);
    let mut last_tv_power_warning = tokio::time::Instant::now() - Duration::from_secs(30);
    let mut hue_retry = RetryState::new();
    let mut nanoleaf_retry = RetryState::new();
    let mut pending_commands = PendingCommands {
        desired_running: (!pipeline_should_run).then_some(false),
        ..PendingCommands::default()
    };
    let mut pipeline_state = if pipeline_should_run {
        PipelineState::Running
    } else {
        PipelineState::Paused
    };

    'sync_loop: while running.load(Ordering::SeqCst) {
        pending_commands.receive(&mut command_rx);
        let loop_start = tokio::time::Instant::now();
        let mut emitted_light_update = false;

        watchdog.tick();

        if auto_tv_power && last_tv_power_poll.elapsed() >= Duration::from_secs(2) {
            last_tv_power_poll = tokio::time::Instant::now();
            match tv_power::is_active().await {
                Ok(state @ Some(_)) => {
                    shared_state.set_tv_power_state(state);
                    let active = state.unwrap();
                    let changed = observed_tv_power
                        .map(|previous| previous != active)
                        .unwrap_or(!active && pipeline_state == PipelineState::Running);
                    if changed {
                        info!(
                            "TV power changed: {}",
                            if active { "active" } else { "standby" }
                        );
                        if pending_commands.desired_running.is_none() {
                            pending_commands.desired_running = Some(active);
                        }
                    }
                    observed_tv_power = Some(active);
                }
                Ok(None) => shared_state.set_tv_power_state(None),
                Err(error) => {
                    shared_state.set_tv_power_state(None);
                    if last_tv_power_warning.elapsed() >= Duration::from_secs(30) {
                        warn!("TV power state unavailable; leaving sync unchanged: {error}");
                        last_tv_power_warning = tokio::time::Instant::now();
                    }
                }
            }
        }

        // 1. Check for web UI or external stop requests
        let web_stop_requested = pending_commands.take_stop_if_running(pipeline_state);
        // Hue external control affects only Hue, never WLED or Nanoleaf.
        // Periodically verify the Hue Entertainment Area ownership (every 5s)
        if last_status_check.elapsed() >= Duration::from_secs(5) {
            last_status_check = tokio::time::Instant::now();
            if hue_active && hue_sync_enabled && hue_dtls.is_some() {
                if let Some(configuration_id) = hue_configuration_id(&config) {
                    if let Ok(state) = hue::get_stream_state(
                        &config.bridge_ip,
                        &config.username,
                        &configuration_id,
                        config.hue_bridge_certificate_sha256.as_deref(),
                    ) {
                        if !state.active {
                            hue_sync_enabled = false;
                            hue_dtls = None;
                            shared_state.hue_connected.store(false, Ordering::Relaxed);
                            info!("Hue external stream stopped; WLED/Nanoleaf remain active");
                        }
                        // The Hue app's smoothing preference is device-specific.
                        // Do not apply it as a second processing pass to all vendors.
                    }
                }
            }
        }

        if web_stop_requested || pipeline_state == PipelineState::Paused {
            info!(
                "Sync was stopped/paused (web={}). Entering paused idle state...",
                web_stop_requested
            );
            shared_state.is_syncing.store(false, Ordering::SeqCst);
            pipeline_state = PipelineState::Paused;
            // Release the LG histogram owner even while waiting in the
            // paused-management loop; never leave VT resources pinned.
            histogram_capture = None;
            using_histogram = false;
            capture = Box::new(MockCapture::new(config.capture_width, config.capture_height));
            shared_state.capture_hardware.store(false, Ordering::Relaxed);
            media_class = PlaybackClass::Unknown;
            drm_router = DrmCaptureRouter::default();
            drm_router.set_mode(config.capture_mode);
            if let Some(task) = media_query.take() { task.abort(); }
            color_pipeline.reset();
            output_gate.clear();
            shared_state.capture_source.store(2, Ordering::Relaxed);
            for (device, _, _) in &mut wled_outputs {
                let black = vec![RgbColor::new(0, 0, 0); device.led_count()];
                let _ = device.send(&LightFrame {
                    colors: &black,
                    source: ColorSource::Frame,
                });
            }

            // Deactivate Hue stream on bridge if web requested stop
            if (web_stop_requested || (auto_tv_power && observed_tv_power == Some(false)))
                && hue_active
            {
                let _ = set_hue_stream_active(&config, false);
            }
            hue_dtls = None;
            shared_state.hue_connected.store(false, Ordering::Relaxed);

            // Fade Nanoleaf to black while paused
            if let Some(ref mut ns) = nanoleaf_streamer {
                let black = vec![RgbColor::new(0, 0, 0); ns.panel_count()];
                let _ = ns.send_frame(&black, 0);
            }
            nanoleaf_streamer = None;
            shared_state
                .nanoleaf_connected
                .store(false, Ordering::Relaxed);

            while running.load(Ordering::SeqCst) {
                tokio::time::sleep(watchdog.pause_poll_period()).await;
                pending_commands.receive(&mut command_rx);
                watchdog.tick();
                if pending_commands.has_management_action()
                    && pending_commands.desired_running != Some(true)
                {
                    break;
                }

                let configured_auto_tv_power =
                    shared_state.current_settings.read().unwrap().auto_tv_power;
                if auto_tv_power != configured_auto_tv_power {
                    auto_tv_power = configured_auto_tv_power;
                    observed_tv_power = None;
                    shared_state.set_tv_power_state(None);
                    last_tv_power_poll = tokio::time::Instant::now() - Duration::from_secs(2);
                }

                let requested_running = pending_commands.desired_running.take();
                if capture_faulted {
                    if requested_running != Some(true) {
                        continue;
                    }
                    // A manual Start retries a fault; TV/app auto-resume must not loop on it.
                    capture_faulted = false;
                    capture_recovery = CaptureRecovery::default();
                    pending_commands.restart = true;
                }
                let mut start_requested = requested_running == Some(true);
                if auto_tv_power && last_tv_power_poll.elapsed() >= Duration::from_secs(2) {
                    last_tv_power_poll = tokio::time::Instant::now();
                    match tv_power::is_active().await {
                        Ok(state @ Some(_)) => {
                            shared_state.set_tv_power_state(state);
                            let active = state.unwrap();
                            if requested_running.is_none()
                                && observed_tv_power == Some(false)
                                && active
                            {
                                start_requested = true;
                            }
                            observed_tv_power = Some(active);
                        }
                        Ok(None) => shared_state.set_tv_power_state(None),
                        Err(error) => {
                            shared_state.set_tv_power_state(None);
                            if last_tv_power_warning.elapsed() >= Duration::from_secs(30) {
                                warn!("TV power state unavailable; leaving sync paused: {error}");
                                last_tv_power_warning = tokio::time::Instant::now();
                            }
                        }
                    }
                }
                let mut app_reactivated = false;
                if !start_requested
                    && hue_active
                    && !(auto_tv_power && observed_tv_power == Some(false))
                {
                    if let Some(configuration_id) = hue_configuration_id(&config) {
                        if let Ok(st) = hue::get_stream_state(
                            &config.bridge_ip,
                            &config.username,
                            &configuration_id,
                            config.hue_bridge_certificate_sha256.as_deref(),
                        ) {
                            if st.active {
                                app_reactivated = true;
                            }
                        }
                    }
                }

                if start_requested || app_reactivated {
                    info!(
                        "Sync resume requested! Reactivating Hue & Nanoleaf streaming sessions..."
                    );
                    let mut hue_resumed = !hue_active || !hue_sync_enabled;
                    if hue_active && hue_sync_enabled {
                        match reconnect_hue(&config) {
                            Ok(client) => {
                                info!("[+] Re-established Hue DTLS streaming session.");
                                hue_dtls = Some(client);
                                shared_state.hue_connected.store(true, Ordering::Relaxed);
                                hue_resumed = true;
                            }
                            Err(e) => {
                                error!("Failed to re-establish Hue DTLS streaming session: {}", e);
                                shared_state.hue_connected.store(false, Ordering::Relaxed);
                            }
                        }
                    }

                    if !hue_resumed
                        && !(config.wled_sync_enabled && !wled_outputs.is_empty())
                        && !(nanoleaf_active && nanoleaf_sync_enabled)
                    {
                        shared_state.is_syncing.store(false, Ordering::SeqCst);
                        continue;
                    }

                    shared_state.is_syncing.store(true, Ordering::SeqCst);
                    pipeline_state = PipelineState::Running;

                    if nanoleaf_active && nanoleaf_sync_enabled {
                        if let Some(ref n_cfg) = config.nanoleaf {
                            match nanoleaf::enable_external_control(&n_cfg.ip, &n_cfg.auth_token) {
                                Ok(port) => {
                                    info!(
                                        "[+] Re-enabled Nanoleaf external control on UDP port {}",
                                        port
                                    );
                                    if let Some(sampler) = nanoleaf_sampler.as_ref() {
                                        match NanoleafUdpStreamer::new(
                                            &n_cfg.ip,
                                            port,
                                            sampler.panel_ids(),
                                        ) {
                                            Ok(new_streamer) => {
                                                nanoleaf_streamer = Some(new_streamer);
                                                shared_state
                                                    .nanoleaf_connected
                                                    .store(true, Ordering::Relaxed);
                                            }
                                            Err(e) => {
                                                shared_state
                                                    .nanoleaf_connected
                                                    .store(false, Ordering::Relaxed);
                                                error!(
                                                    "Failed to recreate Nanoleaf streamer: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to re-enable Nanoleaf external control: {}", e)
                                }
                            }
                        }
                    }

                    break;
                }
            }
            if !running.load(Ordering::SeqCst) {
                break 'sync_loop;
            }
        }

        // 2. Check for live settings update from Web UI
        if let Some(live_st) = pending_commands.apply_settings.take() {
            if auto_tv_power != live_st.auto_tv_power {
                auto_tv_power = live_st.auto_tv_power;
                observed_tv_power = None;
                last_tv_power_poll = tokio::time::Instant::now() - Duration::from_secs(2);
            }
            *shared_state.hue_zones.write().unwrap() = config.zones.clone();
            if hue_sync_enabled != live_st.hue_sync_enabled {
                hue_sync_enabled = live_st.hue_sync_enabled;
                if hue_sync_enabled && pipeline_state == PipelineState::Running {
                    match reconnect_hue(&config) {
                        Ok(client) => {
                            hue_dtls = Some(client);
                            shared_state.hue_connected.store(true, Ordering::Relaxed);
                        }
                        Err(error) => {
                            hue_sync_enabled = false;
                            shared_state.hue_connected.store(false, Ordering::Relaxed);
                            warn!("Hue sync remains off: {}", error);
                        }
                    }
                } else {
                    let _ = set_hue_stream_active(&config, false);
                    hue_dtls = None;
                    shared_state.hue_connected.store(false, Ordering::Relaxed);
                }
            }
            if nanoleaf_sync_enabled != live_st.nanoleaf_sync_enabled {
                nanoleaf_sync_enabled = live_st.nanoleaf_sync_enabled;
                if nanoleaf_sync_enabled && pipeline_state == PipelineState::Running {
                    if let (Some(n_cfg), Some(sampler)) =
                        (config.nanoleaf.as_ref(), nanoleaf_sampler.as_ref())
                    {
                        match nanoleaf::enable_external_control(&n_cfg.ip, &n_cfg.auth_token)
                            .and_then(|port| {
                                NanoleafUdpStreamer::new(&n_cfg.ip, port, sampler.panel_ids())
                            }) {
                            Ok(streamer) => {
                                nanoleaf_streamer = Some(streamer);
                                shared_state
                                    .nanoleaf_connected
                                    .store(true, Ordering::Relaxed);
                            }
                            Err(error) => {
                                nanoleaf_sync_enabled = false;
                                shared_state
                                    .nanoleaf_connected
                                    .store(false, Ordering::Relaxed);
                                warn!("Nanoleaf sync remains off: {}", error);
                            }
                        }
                    }
                } else {
                    nanoleaf_streamer = None;
                    shared_state
                        .nanoleaf_connected
                        .store(false, Ordering::Relaxed);
                }
            }
            color_pipeline.tune(
                live_st.saturation_boost,
                live_st.noise_gate_threshold,
                live_st.gamma,
                live_st.peak_weight,
                live_st.rise_smoothing_factor,
                live_st.fall_smoothing_factor,
                live_st.strict_blackout,
                live_st.max_color_step,
                live_st.hdr_tone_mapping,
                live_st.letterbox_detection,
                live_st.spatial_blend,
            );
            output_gate.clear();
            if let Some(ref mut s) = hue_sampler {
                s.set_temporal_response(
                    live_st.rise_smoothing_factor,
                    live_st.fall_smoothing_factor,
                );
                s.set_strict_blackout(live_st.strict_blackout);
                s.set_hdr_tone_mapping(live_st.hdr_tone_mapping);
                s.set_letterbox_detection(live_st.letterbox_detection);
                s.set_saturation_boost(live_st.saturation_boost);
                s.set_peak_weight(live_st.peak_weight);
                s.set_spatial_blend(live_st.spatial_blend);
                s.set_gamma(live_st.gamma);
                s.set_noise_gate_threshold(live_st.noise_gate_threshold);
                s.set_max_color_step(live_st.max_color_step);
            }
            if let Some(ref mut ns) = nanoleaf_sampler {
                ns.set_temporal_response(
                    live_st.rise_smoothing_factor,
                    live_st.fall_smoothing_factor,
                );
                ns.set_strict_blackout(live_st.strict_blackout);
                ns.set_hdr_tone_mapping(live_st.hdr_tone_mapping);
                ns.set_saturation_boost(live_st.saturation_boost);
                ns.set_brightness_multiplier(
                    live_st.brightness_multiplier * live_st.nanoleaf_output_brightness,
                );
                ns.set_peak_weight(live_st.peak_weight);
                ns.set_spatial_blend(live_st.spatial_blend);
                ns.set_gamma(live_st.gamma);
                ns.set_noise_gate_threshold(live_st.noise_gate_threshold);
                ns.set_max_color_step(live_st.max_color_step);
                ns.set_alignment(live_st.nanoleaf_alignment);
            }
            info!(
                "Applied live settings: Hue={:.1}x, Nanoleaf={:.1}x, saturation={:.1}x, smoothing={:.2}, xy_mode={}",
                live_st.brightness_multiplier * live_st.hue_output_brightness,
                live_st.brightness_multiplier * live_st.nanoleaf_output_brightness,
                live_st.saturation_boost,
                live_st.smoothing_factor,
                live_st.use_xy_gamut
            );
        }

        // 3. Check for save config request from Web UI
        if !pending_commands.save_config.is_empty() {
            let live_st = shared_state.current_settings.read().unwrap().clone();
            let result = Config::update(&config_path, Some(config.clone()), |save_cfg| {
                save_cfg.brightness_multiplier = live_st.brightness_multiplier;
                save_cfg.hue_output_brightness = live_st.hue_output_brightness;
                save_cfg.nanoleaf_output_brightness = live_st.nanoleaf_output_brightness;
                save_cfg.saturation_boost = live_st.saturation_boost;
                save_cfg.peak_weight = live_st.peak_weight;
                save_cfg.spatial_blend = live_st.spatial_blend;
                save_cfg.gamma = live_st.gamma;
                save_cfg.noise_gate_threshold = live_st.noise_gate_threshold;
                save_cfg.smoothing_factor = live_st.smoothing_factor;
                save_cfg.rise_smoothing_factor = live_st.rise_smoothing_factor;
                save_cfg.fall_smoothing_factor = live_st.fall_smoothing_factor;
                save_cfg.strict_blackout = live_st.strict_blackout;
                save_cfg.use_xy_gamut = live_st.use_xy_gamut;
                save_cfg.letterbox_detection = live_st.letterbox_detection;
                save_cfg.hdr_tone_mapping = live_st.hdr_tone_mapping;
                save_cfg.hue_sync_enabled = hue_sync_enabled;
                save_cfg.nanoleaf_sync_enabled = nanoleaf_sync_enabled;
                save_cfg.auto_tv_power = live_st.auto_tv_power;
                save_cfg.max_color_step = live_st.max_color_step;
                Ok(())
            })
            .map(|_| ())
            .map_err(|error| error.to_string());
            if result.is_ok() {
                info!("[+] Successfully saved live settings to {:?}", config_path);
            } else if let Err(ref error) = result {
                error!(
                    "Failed to save updated config to {:?}: {}",
                    config_path, error
                );
            }
            for reply in std::mem::take(&mut pending_commands.save_config) {
                let _ = reply.send(result.clone());
            }
        }

        if std::mem::take(&mut pending_commands.sync_bridge) && hue_active {
            let bridge_identity = (
                config.bridge_ip.clone(),
                config.username.clone(),
                config.hue_bridge_certificate_sha256.clone(),
            );
            match sync_entertainment_areas(
                &config.bridge_ip,
                &config.username,
                config.entertainment_configuration_id.as_deref(),
                config.hue_bridge_certificate_sha256.as_deref(),
            ) {
                Ok(area) if !area.zones.is_empty() => {
                    let configuration_changed = config.entertainment_configuration_id.as_deref()
                        != Some(&area.configuration_id);
                    if configuration_changed {
                        let _ = set_hue_stream_active(&config, false);
                    }
                    config.entertainment_area_id = area.configuration_id.clone();
                    config.entertainment_configuration_id = Some(area.configuration_id);
                    config.hue_bridge_certificate_sha256 = Some(area.certificate_sha256);
                    config.zones = area.zones;
                    *shared_state.hue_zones.write().unwrap() = config.zones.clone();
                    let live_st = shared_state.current_settings.read().unwrap().clone();
                    hue_sampler = Some(ZoneSampler::new(
                        config.zones.clone(),
                        live_st.smoothing_factor,
                        live_st.hdr_tone_mapping,
                        live_st.letterbox_detection,
                        live_st.saturation_boost,
                        live_st.noise_gate_threshold,
                    ));
                    if let Some(ref mut sampler) = hue_sampler {
                        sampler.set_temporal_response(
                            live_st.rise_smoothing_factor,
                            live_st.fall_smoothing_factor,
                        );
                        sampler.set_strict_blackout(live_st.strict_blackout);
                        sampler.set_peak_weight(live_st.peak_weight);
                        sampler.set_spatial_blend(live_st.spatial_blend);
                        sampler.set_gamma(live_st.gamma);
                        sampler.set_max_color_step(live_st.max_color_step);
                    }
                    if configuration_changed {
                        hue_packet_builder =
                            Some(HueStreamPacketBuilder::new(hue_configuration_id(&config)));
                        if hue_sync_enabled && pipeline_state == PipelineState::Running {
                            match reconnect_hue(&config) {
                                Ok(client) => {
                                    hue_dtls = Some(client);
                                    shared_state.hue_connected.store(true, Ordering::Relaxed);
                                }
                                Err(error) => {
                                    hue_dtls = None;
                                    shared_state.hue_connected.store(false, Ordering::Relaxed);
                                    warn!("Hue area changed but DTLS rebind failed: {}", error);
                                }
                            }
                        }
                    }
                    if let Err(error) = Config::update(&config_path, None, |current| {
                        if (
                            current.bridge_ip.as_str(),
                            current.username.as_str(),
                            current.hue_bridge_certificate_sha256.as_ref(),
                        ) != (
                            bridge_identity.0.as_str(),
                            bridge_identity.1.as_str(),
                            bridge_identity.2.as_ref(),
                        ) {
                            anyhow::bail!("Hue Bridge configuration changed during sync; retry");
                        }
                        current.entertainment_area_id = config.entertainment_area_id.clone();
                        current.entertainment_configuration_id =
                            config.entertainment_configuration_id.clone();
                        current.hue_bridge_certificate_sha256 =
                            config.hue_bridge_certificate_sha256.clone();
                        current.zones = config.zones.clone();
                        Ok(())
                    }) {
                        warn!(
                            "Hue area sync succeeded but could not persist its certificate pin: {}",
                            error
                        );
                    }
                    info!(
                        "Synced and persisted {} Hue channel positions from '{}'.",
                        config.zones.len(),
                        area.name
                    );
                }
                Ok(area) => warn!(
                    "Hue area '{}' did not provide channel positions; leaving zones unchanged.",
                    area.name
                ),
                Err(e) => warn!("Hue bridge sync failed; leaving zones unchanged: {}", e),
            }
        }

        // 4. Check for pipeline restart request from Web UI
        if std::mem::take(&mut pending_commands.reconfigure) {
            info!("Device credentials saved; restarting daemon to load the new configuration.");
            break;
        }

        if std::mem::take(&mut pending_commands.restart) {
            info!("Pipeline restart requested via Web UI. Re-initializing capture...");
            drop(std::mem::replace(
                &mut capture,
                Box::new(MockCapture::new(
                    config.capture_width,
                    config.capture_height,
                )),
            ));
            shared_state
                .capture_hardware
                .store(false, Ordering::Relaxed);
            if !runtime::wait_for_capture_retry(Duration::from_millis(500), &shutdown).await {
                break 'sync_loop;
            }
            pending_commands.receive(&mut command_rx);
            if pending_commands.desired_running == Some(false) || pending_commands.reconfigure {
                continue 'sync_loop;
            }
            capture_faulted = false;
            histogram_capture = None;
            using_histogram = false;
            color_pipeline.reset();
            output_gate.clear();
            capture_recovery = CaptureRecovery::default();
            capture_retry.success();
            media_class = PlaybackClass::Unknown;
            drm_router = DrmCaptureRouter::default();
            drm_router.set_mode(config.capture_mode);
            if let Some(task) = media_query.take() { task.abort(); }
            capture = Box::new(MockCapture::new(config.capture_width, config.capture_height));
            shared_state
                .capture_hardware
                .store(capture.is_real_hardware(), Ordering::Relaxed);
        }

        if pipeline_state == PipelineState::Paused {
            histogram_capture = None;
            continue 'sync_loop;
        }

        // Reconnect each configured vendor independently.
        if hue_active && hue_sync_enabled && hue_dtls.is_none() && hue_retry.ready() {
            match reconnect_hue(&config) {
                Ok(client) => {
                    hue_dtls = Some(client);
                    hue_retry.success();
                    shared_state.hue_connected.store(true, Ordering::Relaxed);
                    output_gate.clear();
                }
                Err(err) => {
                    hue_retry.failure();
                    warn!("Hue reconnect unavailable: {err}");
                }
            }
        }
        if nanoleaf_active
            && nanoleaf_sync_enabled
            && nanoleaf_streamer.is_none()
            && nanoleaf_retry.ready()
        {
            if let (Some(cfg), Some(layout)) = (config.nanoleaf.as_ref(), nanoleaf_sampler.as_ref())
            {
                match nanoleaf::enable_external_control(&cfg.ip, &cfg.auth_token)
                    .and_then(|port| NanoleafUdpStreamer::new(&cfg.ip, port, layout.panel_ids()))
                {
                    Ok(transport) => {
                        nanoleaf_streamer = Some(transport);
                        nanoleaf_retry.success();
                        shared_state
                            .nanoleaf_connected
                            .store(true, Ordering::Relaxed);
                        output_gate.clear();
                    }
                    Err(err) => {
                        nanoleaf_retry.failure();
                        warn!("Nanoleaf reconnect unavailable: {err}");
                    }
                }
            }
        }

        // In auto FPS mode, dynamically update cadence if TV source rate changed (e.g. film started)
        if config.fps == 0 {
            if let Some(new_fps) = detect_source_fps() {
                let clamped = new_fps.clamp(20.0, 60.0);
                if (clamped - target_fps).abs() > 0.5 {
                    target_fps = clamped;
                    frame_interval = Duration::from_secs_f64(1.0 / target_fps);
                    shared_state.set_fps(target_fps as f32);
                    info!(
                        "Video source timing changed: adapting sync cadence to {:.2} FPS",
                        target_fps
                    );
                }
            }
        }

        // Query Luna's PRIVATE bus in interactive mode. On C5 the -n 1
        // shortcut exits successfully but emits no response. Run requests
        // in bounded background tasks so a missing service cannot freeze
        // the capture/lighting cadence.
        if model_query.as_ref().is_some_and(|task| task.is_finished()) {
            let completed = model_query.take().expect("checked above");
            tv_model = completed.await.ok().flatten();
            if !config.histogram.matches_tv_model(tv_model.as_deref())
                && last_model_warning.elapsed() >= Duration::from_secs(60)
            {
                warn!("C5 histogram spatial profile not valid for TV model {:?}; refusing spatial mapping", tv_model);
                last_model_warning = tokio::time::Instant::now();
            }
        }
        if model_query.is_none() && last_model_check.elapsed() >= Duration::from_secs(30) {
            last_model_check = tokio::time::Instant::now();
            model_query = Some(tokio::spawn(capture::media::query_tv_model()));
        }

        if media_query.as_ref().is_some_and(|task| task.is_finished()) {
            let completed = media_query.take().expect("checked above");
            media_class = completed.await.unwrap_or(PlaybackClass::Unknown);
            shared_state.playback_class.store(
                match media_class {
                    PlaybackClass::Unknown => 0,
                    PlaybackClass::Clear => 1,
                    PlaybackClass::Protected => 2,
                },
                Ordering::Relaxed,
            );
        }
        if media_query.is_none() && last_media_check.elapsed() >= Duration::from_secs(1) {
            last_media_check = tokio::time::Instant::now();
            media_query = Some(tokio::spawn(capture::media::query()));
        }
        let route = drm_router.decide(
            media_class,
            if histogram_capture.is_some() {
                HistogramHealth::Valid
            } else {
                HistogramHealth::NotReady
            },
            std::time::Instant::now(),
        );
        let needs_histogram = route.source != Source::Frame;
        // Release VT frame ownership as soon as clear classification is lost.
        if needs_histogram && capture.is_real_hardware() {
            capture = Box::new(MockCapture::new(config.capture_width, config.capture_height));
            shared_state.capture_hardware.store(false, Ordering::Relaxed);
            color_pipeline.reset();
            output_gate.clear();
        }
        if needs_histogram {
            if !using_histogram {
                // Exclusive VT owner: drop frame capture before LG histogram init.
                capture = Box::new(MockCapture::new(
                    config.capture_width,
                    config.capture_height,
                ));
                shared_state
                    .capture_hardware
                    .store(false, Ordering::Relaxed);
                using_histogram = true;
                color_pipeline.reset();
                output_gate.clear();
                histogram_black_sent = false;
                last_histogram_valid = None;
                info!("DRM-first source selected: LG histogram; releasing frame capture");
            }
            if route.source == Source::Histogram && histogram_capture.is_none()
                && last_histogram_attempt.elapsed() >= Duration::from_secs(3)
            {
                last_histogram_attempt = tokio::time::Instant::now();
                match HistogramCapture::try_new() {
                    Ok(driver) => {
                        histogram_capture = Some(driver);
                        info!("LG histogram initialized, awaiting populated measurements");
                    }
                    Err(err) => warn!("LG histogram unavailable: {err:#}"),
                }
            }
            let mut new_colors = None;
            if let Some(driver) = histogram_capture.as_mut() {
                match driver.sample() {
                    Ok(colors) => new_colors = Some(colors),
                    Err(err) => {
                        warn!("LG histogram read interrupted: {err:#}");
                        histogram_capture = None;
                        last_histogram_attempt = tokio::time::Instant::now();
                    }
                }
            }

            let processed_field = if let Some(regions) = new_colors {
                if !config.histogram.matches_tv_model(tv_model.as_deref()) {
                    // No inferred spatial map or vendor-specific colour path.
                    None
                } else {
                    match color_pipeline.histogram(&regions, &config.histogram) {
                        Ok(field) => Some(field),
                        Err(error) => {
                            warn!("Rejected invalid histogram colour field: {error:#}");
                            None
                        }
                    }
                }
            } else {
                None
            };
            if let Some(field) = processed_field {
                last_histogram_valid = Some(std::time::Instant::now());
                histogram_black_sent = false;
                shared_state.capture_source.store(1, Ordering::Relaxed);
                let settings = shared_state.current_settings.read().unwrap().clone();
                let mut fanout = OutputFanout {
                    field: &field,
                    config: &config,
                    settings: &settings,
                    shared: &shared_state,
                    wled: &mut wled_outputs,
                    hue_transport: &mut hue_dtls,
                    hue_packets: &mut hue_packet_builder,
                    nanoleaf_transport: &mut nanoleaf_streamer,
                    nanoleaf_layout: &nanoleaf_sampler,
                    hue_enabled: hue_active && hue_sync_enabled,
                    nanoleaf_enabled: nanoleaf_active && nanoleaf_sync_enabled,
                    hue_retry: &mut hue_retry,
                    nanoleaf_retry: &mut nanoleaf_retry,
                    gate: &mut output_gate,
                };
                emitted_light_update = fanout.publish();
            } else {
                shared_state.capture_source.store(2, Ordering::Relaxed);
                // Fail closed, not by sending the synthetic MockCapture's colours.
                // Hold last real colour for <=500ms, then explicitly send black.
                let expired =
                    last_histogram_valid.is_none_or(|t| t.elapsed() > Duration::from_millis(500));
                if expired && !histogram_black_sent {
                    histogram_black_sent = true;
                    output_gate.clear();
                    for (device, _, _) in &mut wled_outputs {
                        let black = vec![RgbColor::new(0, 0, 0); device.led_count()];
                        let _ = device.send(&LightFrame {
                            colors: &black,
                            source: ColorSource::Histogram,
                        });
                    }
                    if let Some(ref mut nl) = nanoleaf_streamer {
                        let black = vec![RgbColor::new(0, 0, 0); nl.panel_count()];
                        let _ = nl.send(&LightFrame {
                            colors: &black,
                            source: ColorSource::Histogram,
                        });
                    }
                    if let (Some(ref mut dtls), Some(ref mut packets)) =
                        (&mut hue_dtls, &mut hue_packet_builder)
                    {
                        let ids: Vec<u8> = config.zones.iter().map(|z| z.channel_id).collect();
                        let black = vec![RgbColor::new(0, 0, 0); ids.len()];
                        let mut output = HueOutput {
                            transport: dtls,
                            packets,
                            channel_ids: &ids,
                            use_xy: false,
                        };
                        let _ = output.send(&LightFrame {
                            colors: &black,
                            source: ColorSource::Histogram,
                        });
                    }
                }
            }
            if emitted_light_update {
                light_update_count = light_update_count.saturating_add(1);
            }
            let update_elapsed = light_update_window.elapsed();
            if update_elapsed >= Duration::from_secs(1) {
                shared_state
                    .set_light_update_fps(light_update_count as f32 / update_elapsed.as_secs_f32());
                light_update_count = 0;
                light_update_window = tokio::time::Instant::now();
            }
            if !runtime::wait_for_capture_retry(frame_interval, &shutdown).await {
                break 'sync_loop;
            }
            continue 'sync_loop;
        }

        if using_histogram {
            // Confirm histogram resource is released before reacquiring scaler.
            histogram_capture = None;
            using_histogram = false;
            color_pipeline.reset();
            output_gate.clear();
            histogram_black_sent = false;
            last_histogram_valid = None;
            capture = create_capture(config.capture_width, config.capture_height);
            shared_state
                .capture_hardware
                .store(capture.is_real_hardware(), Ordering::Relaxed);
            capture_retry.success();
            capture_recovery = CaptureRecovery::default();
            info!("Unprotected media stable; returned to normal frame capture");
        }
        shared_state.capture_source.store(0, Ordering::Relaxed);

        // If currently on fallback MockCapture, rapidly probe if hardware VtCapture has become available
        // (e.g. if the TV was on the home screen at boot or during input/format switch)
        if !capture.is_real_hardware() && capture_retry.ready() {
            match VtCapture::try_new(config.capture_width, config.capture_height) {
                Ok(hw_capture) => {
                    info!("[+] Successfully upgraded from MockCapture to hardware VtCapture!");
                    capture = Box::new(hw_capture);
                    capture_retry.success();
                    shared_state.capture_hardware.store(true, Ordering::Relaxed);
                }
                Err(error) => {
                    let delay = capture_retry.failure();
                    warn!("Hardware capture unavailable ({error:#}); retrying in {delay:?}");
                }
            }
        }

        let hardware_frame = capture.is_real_hardware();
        match capture.acquire_frame() {
            Ok(frame) => {
                if hardware_frame {
                    capture_recovery.frame_received(std::time::Instant::now());
                }
                // Processing is independent of which outputs are enabled.
                // A synthetic MockCapture is never published to real lights.
                if hardware_frame {
                    match color_pipeline.frame(frame.data, frame.width, frame.height, frame.is_bgra)
                    {
                        Ok(field) => {
                            let settings = shared_state.current_settings.read().unwrap().clone();
                            let mut fanout = OutputFanout {
                                field: &field,
                                config: &config,
                                settings: &settings,
                                shared: &shared_state,
                                wled: &mut wled_outputs,
                                hue_transport: &mut hue_dtls,
                                hue_packets: &mut hue_packet_builder,
                                nanoleaf_transport: &mut nanoleaf_streamer,
                                nanoleaf_layout: &nanoleaf_sampler,
                                hue_enabled: hue_active && hue_sync_enabled,
                                nanoleaf_enabled: nanoleaf_active && nanoleaf_sync_enabled,
                                hue_retry: &mut hue_retry,
                                nanoleaf_retry: &mut nanoleaf_retry,
                                gate: &mut output_gate,
                            };
                            emitted_light_update = fanout.publish();
                        }
                        Err(error) => {
                            warn!("Rejected malformed capture sample: {error:#}");
                            color_pipeline.reset();
                        }
                    }
                }
            }
            Err(e) => {
                let delay = match capture_recovery.failed(&e) {
                    RecoveryAction::Retry => {
                        if !runtime::wait_for_capture_retry(frame_interval, &shutdown).await {
                            break 'sync_loop;
                        }
                        continue 'sync_loop;
                    }
                    RecoveryAction::Pause => {
                        error!("Capture paused ({e:#}); invalid frame or recovery limit reached. Use Start or Restart to retry.");
                        capture = Box::new(MockCapture::new(
                            config.capture_width,
                            config.capture_height,
                        ));
                        shared_state
                            .capture_hardware
                            .store(false, Ordering::Relaxed);
                        capture_faulted = true;
                        pending_commands.desired_running = Some(false);
                        continue 'sync_loop;
                    }
                    RecoveryAction::Restart(delay) => delay,
                };
                warn!("Capture recovery ({e:#}); releasing hardware scaler, retrying in {delay:?}");
                capture = Box::new(MockCapture::new(
                    config.capture_width,
                    config.capture_height,
                ));
                shared_state
                    .capture_hardware
                    .store(false, Ordering::Relaxed);

                // Clear/black out Nanoleaf during video transition
                if let Some(ref mut ns) = nanoleaf_streamer {
                    let black = vec![RgbColor::new(0, 0, 0); ns.panel_count()];
                    let _ = ns.send_frame(&black, 0);
                }

                if !runtime::wait_for_capture_retry(delay, &shutdown).await {
                    break 'sync_loop;
                }
                watchdog.tick();
                pending_commands.receive(&mut command_rx);
                if pending_commands.desired_running == Some(false) || pending_commands.reconfigure {
                    continue 'sync_loop;
                }

                // Auto-adapt to new source refresh rate if enabled
                if config.fps == 0 {
                    if let Some(new_fps) = detect_source_fps() {
                        let clamped = new_fps.clamp(20.0, 60.0);
                        if (clamped - target_fps).abs() > 0.5 {
                            target_fps = clamped;
                            frame_interval = Duration::from_secs_f64(1.0 / target_fps);
                            shared_state.set_fps(target_fps as f32);
                            info!(
                                "Video source timing changed: adapting sync cadence to {:.2} FPS",
                                target_fps
                            );
                        }
                    }
                }

                // Route again after retry delay; never initialize from stale media state.
                capture = Box::new(MockCapture::new(config.capture_width, config.capture_height));
                shared_state
                    .capture_hardware
                    .store(capture.is_real_hardware(), Ordering::Relaxed);
                capture_retry.failure();
            }
        }

        if emitted_light_update {
            light_update_count = light_update_count.saturating_add(1);
        }
        let output_elapsed = light_update_window.elapsed();
        if output_elapsed >= Duration::from_secs(1) {
            shared_state
                .set_light_update_fps(light_update_count as f32 / output_elapsed.as_secs_f32());
            light_update_count = 0;
            light_update_window = tokio::time::Instant::now();
        }

        let elapsed = loop_start.elapsed();
        if elapsed < frame_interval {
            tokio::time::sleep(frame_interval - elapsed).await;
        }
    }

    // Cleanup: deactivate Hue stream
    if hue_active {
        info!("Deactivating entertainment area stream on Hue Bridge...");
        let _ = set_hue_stream_active(&config, false);
    }

    signal_task.abort();
    let _ = signal_task.await;
    pending_commands.shutdown(command_rx);
    web_shutdown.cancel();
    if let Some(tasks) = web_tasks {
        tasks.close();
        tasks.wait().await;
    }

    info!("lg-hue-sync terminated cleanly.");
    Ok(())
}

async fn run_test_pattern(config_path: PathBuf) -> Result<()> {
    let config = Config::load(&config_path)?;
    info!(
        "Testing connection to Hue Bridge at {}...",
        config.bridge_ip
    );

    set_hue_stream_active(&config, true)?;

    let mut dtls_client =
        HueDtlsClient::connect(&config.bridge_ip, &config.username, &config.clientkey)?;

    let mut packet_builder = HueStreamPacketBuilder::new(hue_configuration_id(&config));
    info!("Streaming fast rainbow test pattern across all Hue entertainment channels for 15 seconds...");

    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 15 {
        let hue_offset = (start.elapsed().as_secs_f32() * 180.0) % 360.0; // Fast rotation (180 deg/sec)
        let mut channels = Vec::new();

        // Broadcast across all 8 channels in the entertainment area (lights + gradient lightstrip)
        for channel_id in 0..8u8 {
            let hue = (hue_offset + (channel_id as f32) * 45.0) % 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
            channels.push((channel_id, RgbColor::new(r, g, b).to_u16()));
        }

        let packet = packet_builder.build_rgb_packet(&channels);
        dtls_client.send(&packet)?;
        tokio::time::sleep(Duration::from_millis(20)).await; // 50 Hz fast cadence
    }

    set_hue_stream_active(&config, false)?;

    info!("[+] Hue test pattern completed successfully!");
    Ok(())
}

async fn run_test_nanoleaf(config_path: PathBuf) -> Result<()> {
    let config = Config::load(&config_path)?;
    let n_cfg = config
        .nanoleaf
        .ok_or_else(|| anyhow!("No Nanoleaf configuration found in {:?}", config_path))?;

    info!("Connecting to Nanoleaf 4D at {}...", n_cfg.ip);
    let udp_port = nanoleaf::enable_external_control(&n_cfg.ip, &n_cfg.auth_token)?;
    let segments = n_cfg.segments.max(30);
    let mut panel_ids = n_cfg.panel_ids.clone();
    if panel_ids.is_empty() {
        panel_ids = (1..=segments).collect();
    }

    let mut streamer = NanoleafUdpStreamer::new(&n_cfg.ip, udp_port, panel_ids)?;
    info!("Streaming rotating rainbow chase around TV perimeter for 10 seconds...");

    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 10 {
        let hue_offset = (start.elapsed().as_secs_f32() * 90.0) % 360.0;
        let mut colors = Vec::with_capacity(segments as usize);

        for i in 0..segments {
            let hue = (hue_offset + (i as f32 / segments as f32) * 360.0) % 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
            colors.push(RgbColor::new(r, g, b));
        }

        streamer.send_frame(&colors, 0)?;
        tokio::time::sleep(Duration::from_millis(33)).await;
    }

    info!("[+] Nanoleaf 4D test pattern completed successfully!");
    Ok(())
}

/// Probe only aggregate RGB histogram counts. This is an opt-in hardware
/// operation: initialization may create a temporary LG developer marker and
/// acquire exclusive VT capture ownership. No LEDs, frames or pixels sent.
async fn run_test_histogram(
    acknowledge_native_capture: bool,
    samples: u8,
    interval_ms: u64,
) -> Result<()> {
    if !acknowledge_native_capture {
        return Err(anyhow!(
            "native capture probe requires --acknowledge-native-capture;              stop other VT owners and review docs/LG_C5_HISTOGRAM_PROBE.md"
        ));
    }
    if !(2..=20).contains(&samples) {
        return Err(anyhow!("samples must be between 2 and 20"));
    }
    if !(100..=2000).contains(&interval_ms) {
        return Err(anyhow!("interval-ms must be between 100 and 2000"));
    }
    info!(
        "Initializing LG native histogram for {} samples, {}ms interval; no light output",
        samples, interval_ms
    );
    let mut capture = HistogramCapture::try_new().context(
        "LG histogram init failed; do not start competing capture or change system flags",
    )?;
    let mut previous: Option<[RgbColor; 16]> = None;
    let mut valid_samples = 0usize;
    let mut failed_samples = 0usize;
    let mut changed_transitions = 0usize;
    for sample_id in 1..=samples {
        match capture.sample() {
            Ok(regions) => {
                valid_samples += 1;
                let changed_regions = previous
                    .as_ref()
                    .map(|last| {
                        last.iter()
                            .zip(regions.iter())
                            .filter(|(a, b)| {
                                a.r.abs_diff(b.r) >= 4
                                    || a.g.abs_diff(b.g) >= 4
                                    || a.b.abs_diff(b.b) >= 4
                            })
                            .count()
                    })
                    .unwrap_or(0);
                if previous.is_some() && changed_regions != 0 {
                    changed_transitions += 1;
                }
                let region_summary = regions
                    .iter()
                    .enumerate()
                    .map(|(index, rgb)| {
                        format!("{index:02}={:03},{:03},{:03}", rgb.r, rgb.g, rgb.b)
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                info!(
                    "HISTOGRAM_SAMPLE {}/{} changed_regions={} {}",
                    sample_id, samples, changed_regions, region_summary
                );
                previous = Some(regions);
            }
            Err(error) => {
                failed_samples += 1;
                warn!(
                    "HISTOGRAM_SAMPLE {}/{} unavailable: {error:#}",
                    sample_id, samples
                );
            }
        }
        if sample_id != samples {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    drop(capture); // finalize LG VT ownership before reporting outcome.
    info!(
        "HISTOGRAM_RESULT valid={} failed={} changed_transitions={}",
        valid_samples, failed_samples, changed_transitions
    );
    if valid_samples < 2 {
        return Err(anyhow!(
            "histogram did not yield two valid samples; check the native error lines"
        ));
    }
    if changed_transitions == 0 {
        warn!("No measurable regional change; scene may be static or histogram input unavailable");
    }
    Ok(())
}

async fn run_test_capture(config_path: PathBuf, raw_nv12_stats: bool) -> Result<()> {
    let config = Config::load(&config_path)?;
    // This test bypasses the live router, so enforce independent clearance.
    let class = capture::media::query().await;
    if class != PlaybackClass::Clear {
        return Err(anyhow!("test-capture requires positively identified clear media, got {class:?}; HDMI Unknown is not clearance"));
    }
    if raw_nv12_stats {
        info!(
            "TV reports {} mode; this does not establish the capture colour space",
            video_mode::current_dynamic_range().await.label()
        );
        let mut capture = VtCapture::try_new(config.capture_width, config.capture_height)
            .context("raw NV12 statistics require available vtCapture hardware")?;
        capture.enable_raw_stats();
        let mut collected = 0;
        for _ in 0..20 {
            let _ = capture.acquire_frame()?;
            if let Some(stats) = capture.take_raw_stats() {
                collected += 1;
                info!(
                    "Raw NV12 centre frame {collected}: Y min/mean/max={}/{:.1}/{}, U mean={:.1}, V mean={:.1}, luma samples={}",
                    stats.y_min,
                    stats.y_mean,
                    stats.y_max,
                    stats.u_mean,
                    stats.v_mean,
                    stats.luma_samples
                );
                if collected == 5 {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        return Err(anyhow!("fewer than five fresh NV12 frames were available"));
    }
    let mut capture = create_capture(config.capture_width, config.capture_height);
    if !capture.is_real_hardware() {
        return Err(anyhow!("test-capture has no validated native frame source (mock capture is not TV data)"));
    }
    let mut sampler = ZoneSampler::new(
        config.zones.clone(),
        0.35,
        config.hdr_tone_mapping,
        config.letterbox_detection,
        config.saturation_boost,
        config.noise_gate_threshold,
    );

    info!("Sampling capture for 5 frames...");
    for frame_idx in 1..=5 {
        let frame = capture.acquire_frame()?;
        let (sampled, is_cut) =
            sampler.sample_frame(frame.data, frame.width, frame.height, frame.is_bgra);
        info!("Frame {} (scene cut: {}):", frame_idx, is_cut);
        for (channel_id, color) in sampled {
            let zone_name = config
                .zones
                .iter()
                .find(|z| z.channel_id == channel_id)
                .map(|z| z.name.as_str())
                .unwrap_or("Unknown");
            info!(
                "  Zone '{}' (channel {}): RGB({}, {}, {})",
                zone_name, channel_id, color.r, color.g, color.b
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

fn watchdog_period(deadline: Duration) -> Duration {
    (deadline / 2).max(Duration::from_millis(1))
}

struct WatchdogHeartbeat {
    period: Option<Duration>,
    last: tokio::time::Instant,
}

impl WatchdogHeartbeat {
    fn new() -> Self {
        Self {
            period: sd_notify::watchdog_enabled().map(watchdog_period),
            last: tokio::time::Instant::now(),
        }
    }

    fn tick(&mut self) {
        if self
            .period
            .is_some_and(|period| self.last.elapsed() >= period)
        {
            let _ = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]);
            self.last = tokio::time::Instant::now();
        }
    }

    fn pause_poll_period(&self) -> Duration {
        self.period
            .unwrap_or(Duration::from_millis(250))
            .min(Duration::from_millis(250))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_output_frames_are_suppressed_but_meaningful_changes_pass() {
        let previous = vec![RgbColor::new(10, 20, 30)];
        let mut gate = OutputGate::default();
        assert!(gate.should_send("hue", &previous, true));
        gate.record("hue", &previous);
        assert!(!gate.should_send("hue", &previous, true));
        assert!(gate.should_send("hue", &[RgbColor::new(13, 20, 30)], true));
    }

    #[test]
    fn watchdog_uses_half_the_reported_deadline() {
        assert_eq!(
            watchdog_period(Duration::from_secs(10)),
            Duration::from_secs(5)
        );
        assert_eq!(watchdog_period(Duration::ZERO), Duration::from_millis(1));
        let idle = WatchdogHeartbeat {
            period: None,
            last: tokio::time::Instant::now(),
        };
        assert_eq!(idle.pause_poll_period(), Duration::from_millis(250));
        let fast = WatchdogHeartbeat {
            period: Some(Duration::from_millis(50)),
            last: tokio::time::Instant::now(),
        };
        assert_eq!(fast.pause_poll_period(), Duration::from_millis(50));
    }
}
