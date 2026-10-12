//! WLED realtime UDP DRGB/DNRGB transport, no Hue bridge or separate server.
use crate::color::RgbColor;
use crate::output::{LightFrame, LightOutput};
use anyhow::Result;
use std::io::{self, Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

pub const DEFAULT_PORT: u16 = 21324;
const DRGB_LIMIT: usize = 490;
const DNRGB_CHUNK: usize = 480;
pub const MAX_LEDS: usize = 65536;
const TIMEOUT_SECS: u8 = 2;

pub fn encode_packets(colors: &[RgbColor]) -> io::Result<Vec<Vec<u8>>> {
    if colors.is_empty() || colors.len() > MAX_LEDS {
        return Err(Error::new(ErrorKind::InvalidInput, "LED count must be 1..=65536"));
    }
    if colors.len() <= DRGB_LIMIT {
        let mut packet = Vec::with_capacity(2 + 3 * colors.len());
        packet.extend_from_slice(&[2, TIMEOUT_SECS]);
        for c in colors { packet.extend_from_slice(&[c.r, c.g, c.b]); }
        return Ok(vec![packet]);
    }
    let mut packets = Vec::with_capacity(colors.len().div_ceil(DNRGB_CHUNK));
    let mut start = 0;
    while start < colors.len() {
        let end = (start + DNRGB_CHUNK).min(colors.len());
        // WLED ignores a DNRGB datagram containing only one colour on some versions.
        let first = if end - start == 1 { start - 1 } else { start };
        let mut packet = Vec::with_capacity(4 + 3 * (end - first));
        packet.extend_from_slice(&[4, TIMEOUT_SECS, (first >> 8) as u8, first as u8]);
        for c in &colors[first..end] { packet.extend_from_slice(&[c.r,c.g,c.b]); }
        packets.push(packet);
        start = end;
    }
    Ok(packets)
}

pub struct WledUdpStreamer {
    socket: UdpSocket,
    target: SocketAddr,
    count: usize,
}
impl WledUdpStreamer {
    pub fn new(host: &str, port: u16, count: usize) -> io::Result<Self> {
        if host.is_empty() || port == 0 || !(1..=MAX_LEDS).contains(&count) {
            return Err(Error::new(ErrorKind::InvalidInput, "invalid WLED host, port or LED count"));
        }
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let target = address.to_socket_addrs()?.next().ok_or_else(|| {
            Error::new(ErrorKind::AddrNotAvailable, "WLED address did not resolve")
        })?;
        let socket = UdpSocket::bind(if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?;
        socket.connect(target)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, target, count })
    }
    pub fn led_count(&self) -> usize { self.count }
    pub fn target(&self) -> SocketAddr { self.target }
}

impl LightOutput for WledUdpStreamer {
    fn kind(&self) -> &'static str { "wled-udp" }
    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        if frame.colors.len() != self.count {
            return Err(Error::new(ErrorKind::InvalidInput, "WLED colour count mismatch").into());
        }
        for packet in encode_packets(frame.colors)? {
            match self.socket.send(&packet) {
                Ok(n) if n == packet.len() => (),
                Ok(_) => return Err(Error::new(ErrorKind::WriteZero, "short WLED UDP datagram").into()),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drgb_and_dnrgb_boundaries() {
        let a = vec![RgbColor::new(1,2,3);490];
        let b = vec![RgbColor::new(1,2,3);491];
        let x = encode_packets(&a).unwrap();
        assert_eq!(x.len(),1);
        assert_eq!(x[0][..5], [2,2,1,2,3]);
        let y = encode_packets(&b).unwrap();
        assert_eq!(y.len(),2);
        assert_eq!(&y[1][..4], &[4,2,1,224]);
        let z = encode_packets(&vec![RgbColor::new(0,0,0);961]).unwrap();
        assert_eq!(z.len(),3);
        assert_eq!(&z[2][..4], &[4,2,3,191]);
    }
    #[test]
    fn rejects_invalid_lengths() {
        assert!(encode_packets(&[]).is_err());
        assert!(encode_packets(&vec![RgbColor::new(0,0,0);65537]).is_err());
    }
}
