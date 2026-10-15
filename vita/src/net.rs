//! Wi-Fi, name lookup and TCP on the Vita.
//!
//! The Vita joins Wi-Fi by itself, from its own Settings, so the app only
//! waits for that. Sockets are std's, set not to block: a call that finds
//! nothing to do says so at once, and the wait is a short sleep and another
//! try, up to a limit. That is the PSP version's way, and std supports it on
//! the Vita, where its socket timeouts are less certain. Names are looked up
//! here over UDP, as on the PSP, so every wait has a limit the app sets.

use crate::report::micros;
use muse_link::{dns, Lost, Stream, Wire};
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::os::fd::AsRawFd;
use std::thread;
use std::time::Duration;
use vitasdk_sys as sys;

const PATIENCE_MS: u32 = 15_000;
/// A send that takes nothing for this long has not been seen to recover,
/// and the question it carried can be asked again sooner.
const STALLED_MS: u32 = 8_000;
const LOOKUP_WAIT_MS: u32 = 1_500;
/// How long a socket with nothing to do is left alone before the next try.
const TRY_EVERY: Duration = Duration::from_millis(5);
/// The most handed to the socket in one call. A real PSP stalled for whole
/// seconds on large uploads until they went out this way.
const SEND_AT_MOST: usize = 512;
const FALLBACK_NAME_SERVERS: [[u8; 4]; 2] = [[1, 1, 1, 1], [8, 8, 8, 8]];
/// The most data in one packet. Full-size packets from a real PSP were lost
/// on the way out while small ones arrived, which stalled every large
/// upload. Any network carries packets this small.
const SEGMENT: i32 = 536;
/// Memory the system's network stack works in.
const NET_MEMORY: usize = 1024 * 1024;

fn step(name: &str, result: i32) -> Result<(), String> {
    if result < 0 {
        return Err(format!("{} failed: {:#x}", name, result));
    }
    Ok(())
}

/// Loads and starts the system's network stack.
pub fn start() -> Result<(), String> {
    unsafe {
        step("load net", sys::sceSysmoduleLoadModule(sys::SCE_SYSMODULE_NET))?;
        let memory: &'static mut [u8] = Vec::leak(vec![0u8; NET_MEMORY]);
        let mut param = sys::SceNetInitParam { memory: memory.as_mut_ptr() as *mut _, size: NET_MEMORY as i32, flags: 0 };
        let started = sys::sceNetInit(&mut param);
        // Busy means another part of the system has started it already.
        if started as u32 != sys::SCE_NET_ERROR_EBUSY {
            step("net init", started)?;
        }
        step("netctl init", sys::sceNetCtlInit())
    }
}

fn wifi_state() -> i32 {
    let mut state = sys::SCE_NETCTL_STATE_DISCONNECTED as i32;
    unsafe { sys::sceNetCtlInetGetState(&mut state) };
    state
}

/// Waits for the Vita to be on Wi-Fi with an address.
pub fn join() -> Result<(), String> {
    let mut last = -1;
    // About 40 seconds, as the PSP version allowed to join.
    for _ in 0..800 {
        let state = wifi_state();
        if state != last {
            crate::say!("wifi state {}", state);
            last = state;
        }
        if state == sys::SCE_NETCTL_STATE_CONNECTED as i32 {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("not on Wi-Fi. Is it on, and joined in Settings?".into())
}

fn info(which: sys::SceNetCtlInfoType) -> Option<sys::SceNetCtlInfo> {
    let mut info: sys::SceNetCtlInfo = unsafe { core::mem::zeroed() };
    (unsafe { sys::sceNetCtlInetGetInfo(which as i32, &mut info) } == 0).then_some(info)
}

fn text(bytes: &[core::ffi::c_char]) -> String {
    let bytes: Vec<u8> = bytes.iter().map(|&byte| byte as u8).take_while(|&byte| byte != 0).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn configured_name_server(which: sys::SceNetCtlInfoType) -> Option<[u8; 4]> {
    let info = info(which)?;
    // Both name server fields are the same 16 bytes of this union.
    let address: Ipv4Addr = text(unsafe { &info.primary_dns }).parse().ok()?;
    (!address.is_unspecified()).then_some(address.octets())
}

/// Runs `attempt` until it has an answer, sleeping between tries, and gives
/// up with `None` after `limit_ms`.
fn until<T>(limit_ms: u32, mut attempt: impl FnMut() -> Option<T>) -> Option<T> {
    let started = micros();
    loop {
        if let Some(answer) = attempt() {
            return Some(answer);
        }
        if (micros() - started) / 1000 >= limit_ms as u64 {
            return None;
        }
        thread::sleep(TRY_EVERY);
    }
}

/// What one try at moving bytes through a socket came to.
enum Tried {
    Moved(usize),
    /// Nothing to read, or no room to write, at this instant.
    Nothing,
    /// The other side has closed. Only a read can find this out.
    Closed,
    Failed(io::Error),
}

impl Tried {
    fn of(result: io::Result<usize>, reading: bool) -> Tried {
        match result {
            Ok(0) if reading => Tried::Closed,
            Ok(0) => Tried::Nothing,
            Ok(count) => Tried::Moved(count),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => Tried::Nothing,
            Err(error) => Tried::Failed(error),
        }
    }
}

pub struct Socket {
    stream: TcpStream,
    closed: bool,
}

impl Socket {
    pub fn connect(address: [u8; 4], port: u16) -> Result<Socket, String> {
        let peer = SocketAddr::from((address, port));
        let stream = TcpStream::connect_timeout(&peer, Duration::from_millis(PATIENCE_MS as u64)).map_err(|error| match error.kind() {
            ErrorKind::TimedOut => String::from("the server did not answer"),
            _ => format!("the server refused the connection ({})", error),
        })?;
        stream.set_nonblocking(true).map_err(|error| format!("non-blocking failed: {}", error))?;
        if let Err(error) = stream.set_nodelay(true) {
            crate::say!("net: could not turn off send gathering: {}", error);
        }
        let socket = Socket { stream, closed: false };
        socket.send_small_packets();
        Ok(socket)
    }

    /// Asked once the connection exists, because the system only lets the
    /// size be made smaller than the one the connection already has.
    fn send_small_packets(&self) {
        let fd = self.stream.as_raw_fd();
        let size = core::mem::size_of::<i32>() as libc::socklen_t;
        let set = unsafe { libc::setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_MAXSEG, &SEGMENT as *const i32 as *const _, size) };
        if set < 0 {
            crate::say!("net: packets of {} refused: {}", SEGMENT, io::Error::last_os_error());
        } else {
            let (mut now, mut length) = (0i32, size);
            unsafe { libc::getsockopt(fd, libc::IPPROTO_TCP, libc::TCP_MAXSEG, &mut now as *mut i32 as *mut _, &mut length) };
            crate::say!("net: sending packets of at most {}", now);
        }
    }

    fn receive(&mut self, into: &mut [u8]) -> Tried {
        let tried = Tried::of(self.stream.read(into), true);
        if let Tried::Closed = tried {
            self.closed = true;
        }
        tried
    }
}

impl Stream for Socket {
    fn read(&mut self, into: &mut [u8]) -> Result<usize, Lost> {
        if into.is_empty() {
            return Ok(0);
        }
        if self.closed {
            return Ok(0);
        }
        let read = until(PATIENCE_MS, || match self.receive(into) {
            Tried::Moved(count) => Some(Ok(count)),
            Tried::Closed => Some(Ok(0)),
            Tried::Nothing => None,
            Tried::Failed(error) => {
                crate::say!("net: read failed: {}", error);
                Some(Err(Lost))
            }
        });
        read.unwrap_or_else(|| {
            crate::say!("net: nothing to read for {} s", PATIENCE_MS / 1000);
            Err(Lost)
        })
    }

    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), Lost> {
        while !bytes.is_empty() {
            let piece = bytes.len().min(SEND_AT_MOST);
            let sent = until(STALLED_MS, || match Tried::of(self.stream.write(&bytes[..piece]), false) {
                Tried::Moved(count) => Some(Ok(count)),
                Tried::Nothing | Tried::Closed => None,
                Tried::Failed(error) => Some(Err(error)),
            });
            match sent {
                Some(Ok(count)) => bytes = &bytes[count..],
                Some(Err(error)) => {
                    crate::say!("net: write failed: {}, {} bytes left", error, bytes.len());
                    return Err(Lost);
                }
                None => {
                    crate::say!("net: no room to write for {} s, {} bytes left", STALLED_MS / 1000, bytes.len());
                    return Err(Lost);
                }
            }
        }
        Ok(())
    }
}

impl Wire for Socket {
    fn readable(&mut self, wait_ms: u32) -> Result<bool, Lost> {
        // A hang-up counts as readable, so the read that follows sees it.
        if self.closed {
            return Ok(true);
        }
        let mut byte = [0u8; 1];
        // A peek leaves the byte for the read that follows.
        let found = until(wait_ms, || match Tried::of(self.stream.peek(&mut byte), true) {
            Tried::Moved(_) | Tried::Closed => Some(Ok(true)),
            Tried::Nothing => None,
            Tried::Failed(_) => Some(Err(Lost)),
        });
        found.unwrap_or(Ok(false))
    }
}

fn ask(socket: &UdpSocket, server: [u8; 4], host: &str) -> Option<[u8; 4]> {
    let mut id = [0u8; 2];
    getrandom::getrandom(&mut id).ok()?;
    let id = u16::from_le_bytes(id);
    let mut packet = [0u8; 512];
    let length = dns::query(&mut packet, host, id)?;
    let to = SocketAddr::from((server, 53));
    if socket.send_to(&packet[..length], to).ok()? != length {
        return None;
    }
    let got = until(LOOKUP_WAIT_MS, || match Tried::of(socket.recv_from(&mut packet).map(|(count, _)| count), true) {
        Tried::Moved(count) => Some(Some(count)),
        Tried::Nothing => None,
        Tried::Closed | Tried::Failed(_) => Some(None),
    })??;
    dns::answer(&packet[..got], id)
}

/// The network's name and its signal strength out of 100.
pub fn wifi() -> Option<(String, u8)> {
    let name = text(unsafe { &info(sys::SCE_NETCTL_INFO_GET_SSID)?.ssid });
    let strength = unsafe { info(sys::SCE_NETCTL_INFO_GET_RSSI_PERCENTAGE)?.rssi_percentage };
    Some((name, strength.min(100) as u8))
}

/// The IPv4 address of `host`, asked of the network's name servers and then
/// of two public ones.
pub fn lookup(host: &str) -> Result<[u8; 4], String> {
    let servers: Vec<[u8; 4]> = [sys::SCE_NETCTL_INFO_GET_PRIMARY_DNS, sys::SCE_NETCTL_INFO_GET_SECONDARY_DNS]
        .into_iter()
        .filter_map(configured_name_server)
        .chain(FALLBACK_NAME_SERVERS)
        .collect();
    let socket = UdpSocket::bind(("0.0.0.0", 0)).map_err(|error| format!("socket failed: {}", error))?;
    socket.set_nonblocking(true).map_err(|error| format!("non-blocking failed: {}", error))?;
    for _round in 0..2 {
        for &server in &servers {
            if let Some(address) = ask(&socket, server, host) {
                return Ok(address);
            }
        }
    }
    Err(format!("no name server answered for {}", host))
}
