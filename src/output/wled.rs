//! WLED realtime (DRGB/DNRGB/DRGBW/DNRGBW) and DDP transports.
//! Controller ACKs are not provided by either UDP wire format.
use crate::color::pipeline::ColorField;
use crate::color::RgbColor;
use crate::output::perimeter::Point;
use crate::output::{LightFrame, LightOutput};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

pub const DEFAULT_PORT: u16 = 21324;
pub const DDP_PORT: u16 = 4048;
const DRGB_LIMIT: usize = 490;
const DRGBW_LIMIT: usize = 367;
const DNRGB_CHUNK: usize = 480;
const DNRGBW_CHUNK: usize = 360;
const DDP_PAYLOAD_BYTES: usize = 1440;
pub const MAX_LEDS: usize = 65536;
const TIMEOUT_SECS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WledProtocol {
    #[default]
    Realtime,
    Ddp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WledColorMode {
    #[default]
    Rgb,
    Rgbw,
}

#[allow(dead_code)] // Public legacy helper retained for downstream consumers.
pub fn default_port(protocol: WledProtocol) -> u16 {
    match protocol {
        WledProtocol::Realtime => DEFAULT_PORT,
        WledProtocol::Ddp => DDP_PORT,
    }
}

fn invalid_count(colors: &[RgbColor]) -> bool {
    colors.is_empty() || colors.len() > MAX_LEDS
}

/// The minimum-component extraction is intentionally simple. No calibrated
/// CCT/white-point can be inferred without measurements from the actual strip.
fn rgbw(color: RgbColor, white_mix: f32) -> [u8; 4] {
    let common = color.r.min(color.g).min(color.b);
    let white = (f32::from(common) * white_mix).round() as u8;
    [color.r - white, color.g - white, color.b - white, white]
}

fn append_pixels(packet: &mut Vec<u8>, colors: &[RgbColor], mode: WledColorMode, mix: f32) {
    for &c in colors {
        match mode {
            WledColorMode::Rgb => packet.extend_from_slice(&[c.r, c.g, c.b]),
            WledColorMode::Rgbw => packet.extend_from_slice(&rgbw(c, mix)),
        }
    }
}

/// Builds a complete WLED frame; DDP sequence numbers advance per datagram.
/// Packet boundaries are deliberately kept under normal Ethernet MTU.
pub fn encode_packets_configured(
    colors: &[RgbColor],
    protocol: WledProtocol,
    mode: WledColorMode,
    white_mix: f32,
    sequence: u8,
) -> io::Result<Vec<Vec<u8>>> {
    if invalid_count(colors) || !white_mix.is_finite() || !(0.0..=1.0).contains(&white_mix) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "LED count must be 1..=65536 and white_mix must be 0..=1",
        ));
    }
    let channels = match mode {
        WledColorMode::Rgb => 3,
        WledColorMode::Rgbw => 4,
    };
    let mut packets = Vec::new();
    match protocol {
        WledProtocol::Realtime => {
            let single_max = if channels == 3 {
                DRGB_LIMIT
            } else {
                DRGBW_LIMIT
            };
            if colors.len() <= single_max {
                let mut packet = Vec::with_capacity(2 + channels * colors.len());
                packet.extend_from_slice(&[if channels == 3 { 2 } else { 3 }, TIMEOUT_SECS]);
                append_pixels(&mut packet, colors, mode, white_mix);
                packets.push(packet);
            } else {
                let chunk = if channels == 3 {
                    DNRGB_CHUNK
                } else {
                    DNRGBW_CHUNK
                };
                let mut start = 0;
                while start < colors.len() {
                    let end = (start + chunk).min(colors.len());
                    // A single trailing colour is ignored by some WLED builds.
                    // Resend the prior LED with the final datagram in this case.
                    let first = if end - start == 1 { start - 1 } else { start };
                    let mut packet = Vec::with_capacity(4 + channels * (end - first));
                    packet.extend_from_slice(&[
                        if channels == 3 { 4 } else { 5 },
                        TIMEOUT_SECS,
                        (first >> 8) as u8,
                        first as u8,
                    ]);
                    append_pixels(&mut packet, &colors[first..end], mode, white_mix);
                    packets.push(packet);
                    start = end;
                }
            }
        }
        WledProtocol::Ddp => {
            let chunk = DDP_PAYLOAD_BYTES / channels;
            let mut seq = if sequence == 0 {
                1
            } else {
                ((sequence - 1) % 15) + 1
            };
            let mut start = 0;
            while start < colors.len() {
                let end = (start + chunk).min(colors.len());
                let byte_offset = (start * channels) as u32;
                let data_len = ((end - start) * channels) as u16;
                let mut packet = Vec::with_capacity(10 + usize::from(data_len));
                // Version 1; PUSH only on the final datagram of a frame.
                packet.extend_from_slice(&[
                    if end == colors.len() { 0x41 } else { 0x40 },
                    seq,
                    if channels == 3 { 0x0b } else { 0x1b },
                    1, // DDP display ID
                ]);
                packet.extend_from_slice(&byte_offset.to_be_bytes());
                packet.extend_from_slice(&data_len.to_be_bytes());
                append_pixels(&mut packet, &colors[start..end], mode, white_mix);
                packets.push(packet);
                seq = if seq == 15 { 1 } else { seq + 1 };
                start = end;
            }
        }
    }
    Ok(packets)
}

/// Preserve the exact legacy RGB/DRGB/DNRGB API for existing clients.
#[allow(dead_code)] // Byte-compatible legacy encoder retained for downstream consumers.
pub fn encode_packets(colors: &[RgbColor]) -> io::Result<Vec<Vec<u8>>> {
    encode_packets_configured(colors, WledProtocol::Realtime, WledColorMode::Rgb, 1.0, 1)
}

pub struct WledUdpStreamer {
    socket: UdpSocket,
    target: SocketAddr,
    count: usize,
    protocol: WledProtocol,
    mode: WledColorMode,
    white_mix: f32,
    sequence: u8,
}

impl WledUdpStreamer {
    #[allow(dead_code)] // Legacy constructor: explicit protocol config uses new_configured.
    pub fn new(host: &str, port: u16, count: usize) -> io::Result<Self> {
        Self::new_configured(
            host,
            port,
            count,
            WledProtocol::Realtime,
            WledColorMode::Rgb,
            1.0,
        )
    }

    pub fn new_configured(
        host: &str,
        port: u16,
        count: usize,
        protocol: WledProtocol,
        mode: WledColorMode,
        white_mix: f32,
    ) -> io::Result<Self> {
        if host.trim().is_empty()
            || port == 0
            || !(1..=MAX_LEDS).contains(&count)
            || !white_mix.is_finite()
            || !(0.0..=1.0).contains(&white_mix)
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid WLED parameters",
            ));
        }
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let target = address.to_socket_addrs()?.next().ok_or_else(|| {
            Error::new(ErrorKind::AddrNotAvailable, "WLED address did not resolve")
        })?;
        let socket = UdpSocket::bind(if target.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        socket.connect(target)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            target,
            count,
            protocol,
            mode,
            white_mix,
            sequence: 1,
        })
    }
    /// WLED maps its own configured LED geometry from the shared field;
    /// this never modifies or smooths the upstream colours.
    pub fn map_field(field: &ColorField, positions: &[Point], gain: f32) -> Vec<RgbColor> {
        field
            .points(positions)
            .into_iter()
            .map(|c| c.scale(gain))
            .collect()
    }
    pub fn led_count(&self) -> usize {
        self.count
    }
    pub fn target(&self) -> SocketAddr {
        self.target
    }
    #[allow(dead_code)] // Useful for independent integration consumers.
    pub fn protocol(&self) -> WledProtocol {
        self.protocol
    }
}

impl LightOutput for WledUdpStreamer {
    fn kind(&self) -> &'static str {
        match self.protocol {
            WledProtocol::Realtime => "wled-realtime",
            WledProtocol::Ddp => "wled-ddp",
        }
    }
    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        let _source = frame.source;
        if frame.colors.len() != self.count {
            return Err(Error::new(ErrorKind::InvalidInput, "WLED colour count mismatch").into());
        }
        let packets = encode_packets_configured(
            frame.colors,
            self.protocol,
            self.mode,
            self.white_mix,
            self.sequence,
        )?;
        for packet in packets {
            match self.socket.send(&packet) {
                Ok(n) if n == packet.len() => (),
                Ok(_) => return Err(Error::new(ErrorKind::WriteZero, "short WLED datagram").into()),
                Err(e) => return Err(e.into()),
            }
            self.sequence = if self.sequence == 15 {
                1
            } else {
                self.sequence + 1
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rgb(n: usize) -> Vec<RgbColor> {
        vec![RgbColor::new(12, 9, 6); n]
    }

    #[test]
    fn legacy_packets_are_byte_compatible() {
        let a = encode_packets(&rgb(490)).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(&a[0][..5], &[2, 2, 12, 9, 6]);
        let b = encode_packets(&rgb(491)).unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(&b[1][..4], &[4, 2, 1, 224]);
        let c = encode_packets(&rgb(961)).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(&c[2][..4], &[4, 2, 3, 191]);
    }

    #[test]
    fn rgbw_uses_minimum_component_extraction() {
        assert_eq!(rgbw(RgbColor::new(100, 70, 30), 1.0), [70, 40, 0, 30]);
        assert_eq!(rgbw(RgbColor::new(100, 70, 30), 0.0), [100, 70, 30, 0]);
        let a = encode_packets_configured(
            &rgb(367),
            WledProtocol::Realtime,
            WledColorMode::Rgbw,
            1.0,
            1,
        )
        .unwrap();
        assert_eq!(a[0][..6], [3, 2, 6, 3, 0, 6]);
        let b = encode_packets_configured(
            &rgb(368),
            WledProtocol::Realtime,
            WledColorMode::Rgbw,
            1.0,
            1,
        )
        .unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(&b[0][..4], &[5, 2, 0, 0]);
        assert_eq!(&b[1][..4], &[5, 2, 1, 104]);
    }

    #[test]
    fn ddp_frame_offsets_push_and_sequence() {
        let packets =
            encode_packets_configured(&rgb(481), WledProtocol::Ddp, WledColorMode::Rgb, 1.0, 15)
                .unwrap();
        assert_eq!(packets.len(), 2);
        assert_eq!(&packets[0][..10], &[0x40, 15, 0x0b, 1, 0, 0, 0, 0, 5, 160]);
        assert_eq!(&packets[1][..10], &[0x41, 1, 0x0b, 1, 0, 0, 5, 160, 0, 3]);
        assert_eq!(packets[0].len(), 1450);
        assert_eq!(packets[1].len(), 13);
    }

    #[test]
    fn ddp_rgbw_uses_channel_offsets() {
        let packets =
            encode_packets_configured(&rgb(361), WledProtocol::Ddp, WledColorMode::Rgbw, 1.0, 1)
                .unwrap();
        assert_eq!(&packets[0][..10], &[0x40, 1, 0x1b, 1, 0, 0, 0, 0, 5, 160]);
        assert_eq!(&packets[1][..10], &[0x41, 2, 0x1b, 1, 0, 0, 5, 160, 0, 4]);
    }

    #[test]
    fn rejects_invalid_parameters() {
        assert!(encode_packets(&[]).is_err());
        assert!(encode_packets(&rgb(65537)).is_err());
        assert!(encode_packets_configured(
            &rgb(2),
            WledProtocol::Ddp,
            WledColorMode::Rgbw,
            f32::NAN,
            1
        )
        .is_err());
        assert!(encode_packets_configured(
            &rgb(2),
            WledProtocol::Ddp,
            WledColorMode::Rgbw,
            1.01,
            1
        )
        .is_err());
        assert_eq!(default_port(WledProtocol::Realtime), 21324);
        assert_eq!(default_port(WledProtocol::Ddp), 4048);
    }
}
