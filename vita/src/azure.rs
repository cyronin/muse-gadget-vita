//! A reply turned into speech by Azure AI Speech, delivered as it arrives,
//! in the same 24 kHz, 16-bit mono form as link's OpenAI speech, so the
//! speaker plays either the same way.
//!
//! link/ holds the OpenAI request and stays as the PSP port left it, so this
//! request is written here, on link's TLS. Azure's speech hosts chain to
//! DigiCert Global Root G2, one of the two roots link trusts.

use crate::{clock, muse};
use muse_link::secure::{self, Secure};
use muse_link::{speech, Net};
use std::sync::Arc;

/// A voice every Azure region offers. `state/azure_voice.txt` can name another.
pub const DEFAULT_VOICE: &str = "en-US-JennyNeural";
const PATH: &str = "/cognitiveservices/v1";
/// Raw samples at link's speech rate, so nothing has to be decoded.
const FORMAT: &str = "raw-24khz-16bit-mono-pcm";
const LARGEST_HEAD: usize = 16 * 1024;

/// What is needed to ask Azure: the resource's key and its region, such as
/// "eastus".
pub struct Account {
    pub key: String,
    pub region: String,
    pub voice: String,
}

/// `text` made safe to put inside SSML.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// The end of the response head: the byte after its empty line.
fn head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n").map(|at| at + 4)
}

fn status(head: &str) -> Option<u16> {
    head.lines().next()?.split_ascii_whitespace().nth(1)?.parse().ok()
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).filter_map(|line| line.split_once(':')).filter(|(key, _)| key.trim().eq_ignore_ascii_case(name)).last().map(|(_, value)| value.trim())
}

/// Where a response body stands. HTTP sends it whole, counted, or in chunks
/// that each say their own size. The same as link's, which is private to it.
#[derive(Debug, PartialEq)]
enum Body {
    UntilClose,
    Counted(usize),
    ChunkSize(usize, bool),
    ChunkSkipLine(usize),
    Chunk(usize),
    ChunkEnd(usize),
    Done,
}

impl Body {
    fn of(head: &str) -> Result<Body, String> {
        if header(head, "transfer-encoding").is_some_and(|coding| coding.eq_ignore_ascii_case("chunked")) {
            return Ok(Body::ChunkSize(0, false));
        }
        match header(head, "content-length") {
            Some(length) => length.parse().map(Body::Counted).map_err(|_| "a garbled length".into()),
            None => Ok(Body::UntilClose),
        }
    }

    /// Passes the body bytes in `wire` to `sink`.
    fn feed(&mut self, mut wire: &[u8], sink: &mut dyn FnMut(&[u8])) -> Result<(), String> {
        while !wire.is_empty() {
            let used = match self {
                Body::Done => return Ok(()),
                Body::UntilClose => {
                    sink(wire);
                    wire.len()
                }
                Body::Counted(left) | Body::Chunk(left) => {
                    let take = wire.len().min(*left);
                    sink(&wire[..take]);
                    *left -= take;
                    if *left == 0 {
                        *self = if matches!(self, Body::Chunk(_)) { Body::ChunkEnd(2) } else { Body::Done };
                    }
                    take
                }
                Body::ChunkSize(size, any) => {
                    match (wire[0], (wire[0] as char).to_digit(16)) {
                        (_, Some(digit)) => {
                            *size = size.checked_mul(16).and_then(|size| size.checked_add(digit as usize)).ok_or("a garbled chunk")?;
                            *any = true;
                        }
                        (b'\r' | b';', _) if *any => *self = Body::ChunkSkipLine(*size),
                        _ => return Err("a garbled chunk".into()),
                    }
                    1
                }
                Body::ChunkSkipLine(size) => {
                    if wire[0] == b'\n' {
                        *self = if *size == 0 { Body::Done } else { Body::Chunk(*size) };
                    }
                    1
                }
                Body::ChunkEnd(left) => {
                    *left -= 1;
                    if *left == 0 {
                        *self = Body::ChunkSize(0, false);
                    }
                    1
                }
            };
            wire = &wire[used..];
        }
        Ok(())
    }
}

/// Speaks `text`. `heard` is given the audio in pieces as it arrives, at
/// link's speech rate. A piece can end in the middle of a sample.
pub fn speak(account: &Account, text: &str, heard: &mut dyn FnMut(&[u8])) -> Result<(), String> {
    let host = format!("{}.tts.speech.microsoft.com", account.region);
    let ssml = format!(
        "<speak version='1.0' xml:lang='en-US'><voice name='{}'>{}</voice></speak>",
        escaped(&account.voice),
        escaped(speech::spoken_part(text))
    );
    let head = format!(
        "POST {PATH} HTTP/1.1\r\nHost: {host}\r\nOcp-Apim-Subscription-Key: {}\r\nContent-Type: application/ssml+xml\r\nX-Microsoft-OutputFormat: {FORMAT}\r\nUser-Agent: muse-gadget-vita\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        account.key,
        ssml.len()
    );
    let settings = secure::settings(Arc::new(clock::Rtc)).map_err(|error| format!("{:?}", error))?;
    let stream = muse::Vita.open(&host, 443).map_err(|_| String::from("Azure could not be reached"))?;
    let mut secure = Secure::open(stream, &host, settings).map_err(|error| format!("{:?}", error))?;
    let failed = |error: secure::Error| format!("{:?}", error);
    secure.write_all(head.as_bytes()).map_err(failed)?;
    secure.write_all(ssml.as_bytes()).map_err(failed)?;

    let mut buffer = vec![0u8; 4096];
    let mut filled = 0;
    let (mut body, start) = loop {
        if filled == buffer.len() {
            if buffer.len() >= LARGEST_HEAD {
                return Err("Azure's answer has no end to its head".into());
            }
            buffer.resize(buffer.len() * 2, 0);
        }
        let got = secure.read(&mut buffer[filled..]).map_err(failed)?;
        if got == 0 {
            return Err("Azure hung up before answering".into());
        }
        filled += got;
        if let Some(end) = head_end(&buffer[..filled]) {
            let head = std::str::from_utf8(&buffer[..end]).map_err(|_| String::from("a garbled answer"))?;
            match status(head) {
                Some(200) => break (Body::of(head)?, end),
                // 401 is a wrong key, or a key from another region.
                Some(code) => return Err(format!("Azure refused with {}", code)),
                None => return Err("a garbled answer".into()),
            }
        }
    };
    body.feed(&buffer[start..filled], heard)?;
    while body != Body::Done {
        let got = secure.read(&mut buffer).map_err(failed)?;
        if got == 0 {
            return if body == Body::UntilClose { Ok(()) } else { Err("Azure hung up mid-answer".into()) };
        }
        body.feed(&buffer[..got], heard)?;
    }
    Ok(())
}
