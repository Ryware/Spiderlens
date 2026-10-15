//! NTLMv2 sign-in (spec 025 plan D4, research R4).
//!
//! NTLM authenticates a connection, not a request: the negotiate,
//! challenge and authenticate messages must travel on one keep-alive
//! connection. reqwest picks pooled connections itself, so each NTLM
//! "slot" is its own client allowed one idle HTTP/1.1 connection, used by
//! one request at a time. A slot's connection signs in once and then serves
//! requests until the server closes it; the next request on that slot
//! signs in again. Only the client side of NTLMv2 is here, without message
//! signing or channel binding (plan risk 2).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest as _, Md5};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::sync::{Mutex, OwnedMutexGuard};

use super::auth::challenges_of;

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";
const NEGOTIATE_UNICODE: u32 = 0x0000_0001;
const NEGOTIATE_OEM: u32 = 0x0000_0002;
const REQUEST_TARGET: u32 = 0x0000_0004;
const NEGOTIATE_NTLM: u32 = 0x0000_0200;
const NEGOTIATE_ALWAYS_SIGN: u32 = 0x0000_8000;
const NEGOTIATE_EXTENDED_SESSIONSECURITY: u32 = 0x0008_0000;
const NEGOTIATE_FLAGS: u32 = NEGOTIATE_UNICODE
    | NEGOTIATE_OEM
    | REQUEST_TARGET
    | NEGOTIATE_NTLM
    | NEGOTIATE_ALWAYS_SIGN
    | NEGOTIATE_EXTENDED_SESSIONSECURITY;
/// MsvAvTimestamp in the server's target information.
const AV_TIMESTAMP: u16 = 7;

// ---- MD4 (RFC 1320), needed for the NT hash only ----

pub(crate) fn md4(input: &[u8]) -> [u8; 16] {
    let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
    let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
    let h = |x: u32, y: u32, z: u32| x ^ y ^ z;

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut s: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for block in msg.chunks_exact(64) {
        let mut x = [0u32; 16];
        for (i, w) in x.iter_mut().enumerate() {
            *w = u32::from_le_bytes([block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3]]);
        }
        let [mut a, mut b, mut c, mut d] = s;

        for &i in &[0usize, 4, 8, 12] {
            a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
            d = d.wrapping_add(f(a, b, c)).wrapping_add(x[i + 1]).rotate_left(7);
            c = c.wrapping_add(f(d, a, b)).wrapping_add(x[i + 2]).rotate_left(11);
            b = b.wrapping_add(f(c, d, a)).wrapping_add(x[i + 3]).rotate_left(19);
        }
        for &i in &[0usize, 1, 2, 3] {
            let k = 0x5a82_7999u32;
            a = a.wrapping_add(g(b, c, d)).wrapping_add(x[i]).wrapping_add(k).rotate_left(3);
            d = d.wrapping_add(g(a, b, c)).wrapping_add(x[i + 4]).wrapping_add(k).rotate_left(5);
            c = c.wrapping_add(g(d, a, b)).wrapping_add(x[i + 8]).wrapping_add(k).rotate_left(9);
            b = b.wrapping_add(g(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k).rotate_left(13);
        }
        for &i in &[0usize, 2, 1, 3] {
            let k = 0x6ed9_eba1u32;
            a = a.wrapping_add(h(b, c, d)).wrapping_add(x[i]).wrapping_add(k).rotate_left(3);
            d = d.wrapping_add(h(a, b, c)).wrapping_add(x[i + 8]).wrapping_add(k).rotate_left(9);
            c = c.wrapping_add(h(d, a, b)).wrapping_add(x[i + 4]).wrapping_add(k).rotate_left(11);
            b = b.wrapping_add(h(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k).rotate_left(15);
        }

        s[0] = s[0].wrapping_add(a);
        s[1] = s[1].wrapping_add(b);
        s[2] = s[2].wrapping_add(c);
        s[3] = s[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, w) in s.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

pub(crate) fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..16].copy_from_slice(&Md5::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Md5::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Md5::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner);
    let mut out = [0u8; 16];
    out.copy_from_slice(&outer.finalize());
    out
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// NTOWFv2 (MS-NLMP 3.3.2).
pub(crate) fn ntowf_v2(user: &str, domain: &str, password: &str) -> [u8; 16] {
    let nt_hash = md4(&utf16le(password));
    hmac_md5(&nt_hash, &utf16le(&format!("{}{domain}", user.to_uppercase())))
}

/// `DOMAIN\user` or `user@domain` into (user, domain).
pub(crate) fn split_user(user: &str) -> (String, String) {
    if let Some((d, u)) = user.split_once('\\') {
        return (u.to_string(), d.to_string());
    }
    (user.to_string(), String::new())
}

/// The negotiate message (type 1), without domain or workstation.
pub(crate) fn negotiate() -> Vec<u8> {
    let mut m = Vec::with_capacity(32);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&1u32.to_le_bytes());
    m.extend_from_slice(&NEGOTIATE_FLAGS.to_le_bytes());
    m.extend_from_slice(&[0u8; 16]);
    m
}

/// The parts of a challenge message (type 2) the answer needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerChallenge {
    pub challenge: [u8; 8],
    pub flags: u32,
    pub target_info: Vec<u8>,
}

fn buffer(m: &[u8], at: usize) -> Option<&[u8]> {
    let len = u16::from_le_bytes(m.get(at..at + 2)?.try_into().ok()?) as usize;
    let offset = u32::from_le_bytes(m.get(at + 4..at + 8)?.try_into().ok()?) as usize;
    m.get(offset..offset.checked_add(len)?)
}

pub(crate) fn parse_challenge(m: &[u8]) -> Option<ServerChallenge> {
    if m.get(..8)? != SIGNATURE || u32::from_le_bytes(m.get(8..12)?.try_into().ok()?) != 2 {
        return None;
    }
    let flags = u32::from_le_bytes(m.get(20..24)?.try_into().ok()?);
    let challenge: [u8; 8] = m.get(24..32)?.try_into().ok()?;
    let target_info = if m.len() >= 48 {
        buffer(m, 40)?.to_vec()
    } else {
        Vec::new()
    };
    Some(ServerChallenge {
        challenge,
        flags,
        target_info,
    })
}

/// The server's timestamp from the target information, if it sent one.
fn av_timestamp(info: &[u8]) -> Option<[u8; 8]> {
    let mut at = 0;
    while at + 4 <= info.len() {
        let id = u16::from_le_bytes([info[at], info[at + 1]]);
        let len = u16::from_le_bytes([info[at + 2], info[at + 3]]) as usize;
        let value = info.get(at + 4..at + 4 + len)?;
        if id == 0 {
            return None;
        }
        if id == AV_TIMESTAMP && len == 8 {
            return value.try_into().ok();
        }
        at += 4 + len;
    }
    None
}

/// Now as a Windows FILETIME: 100 ns ticks since 1601-01-01.
fn filetime_now() -> [u8; 8] {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let since_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = (since_unix.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(since_unix.subsec_nanos() / 100);
    ticks.to_le_bytes()
}

/// The NTLMv2 responses (MS-NLMP 3.3.2): (LMv2, NTv2).
pub(crate) fn responses(
    key: &[u8; 16],
    server: &ServerChallenge,
    client_challenge: [u8; 8],
    timestamp: [u8; 8],
) -> (Vec<u8>, Vec<u8>) {
    let mut temp = Vec::with_capacity(28 + server.target_info.len() + 4);
    temp.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
    temp.extend_from_slice(&timestamp);
    temp.extend_from_slice(&client_challenge);
    temp.extend_from_slice(&[0; 4]);
    temp.extend_from_slice(&server.target_info);
    temp.extend_from_slice(&[0; 4]);

    let mut proof_input = server.challenge.to_vec();
    proof_input.extend_from_slice(&temp);
    let nt_proof = hmac_md5(key, &proof_input);
    let mut nt = nt_proof.to_vec();
    nt.extend_from_slice(&temp);

    let mut lm_input = server.challenge.to_vec();
    lm_input.extend_from_slice(&client_challenge);
    let mut lm = hmac_md5(key, &lm_input).to_vec();
    lm.extend_from_slice(&client_challenge);
    (lm, nt)
}

/// The authenticate message (type 3) for `user` (`DOMAIN\user` allowed).
pub(crate) fn authenticate(user: &str, password: &str, server: &ServerChallenge) -> Vec<u8> {
    let mut client_challenge = [0u8; 8];
    if getrandom::fill(&mut client_challenge).is_err() {
        client_challenge.iter_mut().for_each(|b| *b = fastrand::u8(..));
    }
    let timestamp = av_timestamp(&server.target_info).unwrap_or_else(filetime_now);
    authenticate_with(user, password, server, client_challenge, timestamp, "")
}

pub(crate) fn authenticate_with(
    user: &str,
    password: &str,
    server: &ServerChallenge,
    client_challenge: [u8; 8],
    timestamp: [u8; 8],
    workstation: &str,
) -> Vec<u8> {
    let (user, domain) = split_user(user);
    let key = ntowf_v2(&user, &domain, password);
    let (lm, nt) = responses(&key, server, client_challenge, timestamp);
    let unicode = server.flags & NEGOTIATE_UNICODE != 0;
    let text = |s: &str| if unicode { utf16le(s) } else { s.as_bytes().to_vec() };
    let payloads = [lm, nt, text(&domain), text(&user), text(workstation), Vec::new()];

    const HEADER: usize = 64;
    let mut m = Vec::with_capacity(HEADER + payloads.iter().map(Vec::len).sum::<usize>());
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&3u32.to_le_bytes());
    let mut offset = HEADER;
    for p in &payloads {
        let len = p.len() as u16;
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += p.len();
    }
    let flags = NEGOTIATE_FLAGS & (server.flags | NEGOTIATE_NTLM | REQUEST_TARGET);
    m.extend_from_slice(&flags.to_le_bytes());
    for p in &payloads {
        m.extend_from_slice(p);
    }
    m
}

fn header(message: &[u8]) -> HeaderValue {
    let mut v = HeaderValue::from_str(&format!("NTLM {}", B64.encode(message))).expect("base64 is a valid header");
    v.set_sensitive(true);
    v
}

fn server_challenge(headers: &HeaderMap) -> Option<ServerChallenge> {
    challenges_of(headers)
        .into_iter()
        .find(|c| c.scheme == "ntlm" && c.token.is_some())
        .and_then(|c| B64.decode(c.token?.trim()).ok())
        .and_then(|m| parse_challenge(&m))
}

/// One NTLM connection: a client that keeps at most one idle connection.
/// Holding its guard reserves the connection; keep it until the body is
/// read.
pub(crate) struct Slot {
    client: Client,
    signed_in: bool,
}

/// The NTLM connections of one origin.
pub(crate) struct NtlmPool {
    slots: Vec<Arc<Mutex<Slot>>>,
    next: AtomicUsize,
    user: String,
    password: String,
}

impl NtlmPool {
    /// `clients` are built by the session (same timeouts, proxy and
    /// cookies as every other request), each with
    /// `pool_max_idle_per_host(1)` and HTTP/1.1 only.
    pub fn new(clients: Vec<Client>, user: String, password: String) -> Self {
        Self {
            slots: clients
                .into_iter()
                .map(|client| {
                    Arc::new(Mutex::new(Slot {
                        client,
                        signed_in: false,
                    }))
                })
                .collect(),
            next: AtomicUsize::new(0),
            user,
            password,
        }
    }

    async fn reserve(&self) -> OwnedMutexGuard<Slot> {
        for slot in &self.slots {
            if let Ok(g) = Arc::clone(slot).try_lock_owned() {
                return g;
            }
        }
        let i = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        Arc::clone(&self.slots[i]).lock_owned().await
    }

    /// Sends a GET built by `build`, signing the connection in first when
    /// it is not yet. Returns the final response with the connection still
    /// reserved, and whether credentials were refused.
    pub async fn send(
        &self,
        build: impl Fn(&Client) -> RequestBuilder,
    ) -> Result<(Response, OwnedMutexGuard<Slot>, bool), reqwest::Error> {
        let mut slot = self.reserve().await;
        if slot.signed_in {
            let response = build(&slot.client).send().await?;
            if response.status() != StatusCode::UNAUTHORIZED {
                return Ok((response, slot, false));
            }
            // The server closed the signed-in connection; sign in again.
            let _ = response.bytes().await;
            slot.signed_in = false;
        }
        let first = build(&slot.client).header(AUTHORIZATION, header(&negotiate())).send().await?;
        let Some(challenge) = (first.status() == StatusCode::UNAUTHORIZED)
            .then(|| server_challenge(first.headers()))
            .flatten()
        else {
            // No NTLM challenge: the answer is the answer (a public page,
            // or a server that refuses NTLM outright).
            let refused = first.status() == StatusCode::UNAUTHORIZED;
            return Ok((first, slot, refused));
        };
        // Read the challenge's body so the connection is reused.
        let _ = first.bytes().await;
        let message = authenticate(&self.user, &self.password, &challenge);
        let response = build(&slot.client).header(AUTHORIZATION, header(&message)).send().await?;
        let refused = response.status() == StatusCode::UNAUTHORIZED;
        slot.signed_in = !refused;
        Ok((response, slot, refused))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn md4_matches_rfc_1320() {
        assert_eq!(md4(b"").to_vec(), from_hex("31d6cfe0d16ae931b73c59d7e0c089c0"));
        assert_eq!(md4(b"abc").to_vec(), from_hex("a448017aaf21d8525fc10ae87aa6729d"));
        assert_eq!(
            md4(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890").to_vec(),
            from_hex("e33b4ddc9c38f2199c3e7b164fcc0536")
        );
    }

    #[test]
    fn hmac_md5_matches_rfc_2104() {
        assert_eq!(
            hmac_md5(b"Jefe", b"what do ya want for nothing?").to_vec(),
            from_hex("750c783e6ab0b503eaa86e310a5db738")
        );
    }

    /// MS-NLMP 4.2.4: user "User", domain "Domain", password "Password".
    #[test]
    fn ntlmv2_matches_ms_nlmp_test_vectors() {
        assert_eq!(md4(&utf16le("Password")).to_vec(), from_hex("a4f49c406510bdcab6824ee7c30fd852"));
        let key = ntowf_v2("User", "Domain", "Password");
        assert_eq!(key.to_vec(), from_hex("0c868a403bfd7a93a3001ef22ef02e3f"));

        let mut info = Vec::new();
        for (id, value) in [(2u16, "Domain"), (1, "Server")] {
            let v = utf16le(value);
            info.extend_from_slice(&id.to_le_bytes());
            info.extend_from_slice(&(v.len() as u16).to_le_bytes());
            info.extend_from_slice(&v);
        }
        info.extend_from_slice(&[0, 0, 0, 0]);
        let server = ServerChallenge {
            challenge: from_hex("0123456789abcdef").try_into().unwrap(),
            flags: 0xe28a_8233,
            target_info: info,
        };
        let (lm, nt) = responses(&key, &server, [0xaa; 8], [0; 8]);
        assert_eq!(lm, from_hex("86c35097ac9cec102554764a57cccc19aaaaaaaaaaaaaaaa"));
        assert_eq!(nt[..16].to_vec(), from_hex("68cd0ab851e51c96aabc927bebef6a1c"));
    }

    #[test]
    fn messages_round_trip_through_their_own_parser() {
        let n = negotiate();
        assert_eq!(&n[..8], SIGNATURE);
        assert_eq!(n.len(), 32);

        let mut c = SIGNATURE.to_vec();
        c.extend_from_slice(&2u32.to_le_bytes());
        c.extend_from_slice(&[0, 0, 0, 0, 48, 0, 0, 0]);
        c.extend_from_slice(&NEGOTIATE_FLAGS.to_le_bytes());
        c.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        c.extend_from_slice(&[0; 8]);
        c.extend_from_slice(&[4, 0, 4, 0, 48, 0, 0, 0]);
        c.extend_from_slice(&[0, 0, 0, 0]);
        let parsed = parse_challenge(&c).unwrap();
        assert_eq!(parsed.challenge, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(parsed.target_info, [0, 0, 0, 0]);

        let m = authenticate_with("CORP\\ann", "pw", &parsed, [0xaa; 8], [0; 8], "");
        assert_eq!(&m[..8], SIGNATURE);
        assert_eq!(u32::from_le_bytes(m[8..12].try_into().unwrap()), 3);
        // Domain and user, in UTF-16.
        assert_eq!(buffer(&m, 28).unwrap(), utf16le("CORP"));
        assert_eq!(buffer(&m, 36).unwrap(), utf16le("ann"));
    }

    #[test]
    fn user_forms() {
        assert_eq!(split_user("CORP\\ann"), ("ann".into(), "CORP".into()));
        assert_eq!(split_user("ann"), ("ann".into(), String::new()));
    }
}
