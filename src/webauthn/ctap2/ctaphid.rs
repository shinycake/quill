//! CTAPHID (CTAP 2.1 section 11.2): messages cut into HID reports, a
//! channel per client, keepalives while the key waits for a touch, and
//! CTAPHID_CANCEL.
#![deny(clippy::indexing_slicing)]

use super::{CtapError, status};
use rand::Rng;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const CMD_PING: u8 = 0x01;
pub const CMD_INIT: u8 = 0x06;
pub const CMD_CBOR: u8 = 0x10;
pub const CMD_CANCEL: u8 = 0x11;
pub const CMD_KEEPALIVE: u8 = 0x3B;
pub const CMD_ERROR: u8 = 0x3F;
pub const BROADCAST_CID: u32 = 0xFFFF_FFFF;

/// Keepalive status: the key waits for the user to touch it.
pub const KEEPALIVE_UP_NEEDED: u8 = 2;

/// How long one read waits before the loop looks at the cancel flag.
const READ_SLICE: Duration = Duration::from_millis(100);
/// How long a cancelled request waits for the key to confirm the cancel.
const CANCEL_GRACE: Duration = Duration::from_millis(1500);

/// A FIDO HID device: fixed-size reports, without the report id byte.
pub trait HidDevice: Send {
    /// Bytes per report (64 for every key Quill has seen).
    fn report_len(&self) -> usize;
    /// Send one output report of `report_len` bytes.
    fn write_report(&mut self, report: &[u8]) -> Result<(), HidError>;
    /// Wait up to `timeout` for one input report.
    fn read_report(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, HidError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidError {
    /// The OS refused a read or write; the text is for the log.
    Io(String),
    /// The request ran past its deadline.
    Timeout,
    /// The user cancelled.
    Cancelled,
    /// The key broke the framing rules.
    Protocol(&'static str),
    /// CTAPHID_ERROR with this code.
    Device(u8),
    /// The message does not fit in 128 continuation packets.
    TooLong,
}

/// What a running request watches: the cancel flag, its deadline, and
/// who hears the keepalives.
pub struct Control<'a> {
    pub cancel: &'a AtomicBool,
    pub deadline: Instant,
    pub on_keepalive: &'a mut dyn FnMut(u8),
}

impl Control<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// The reports carrying one message: an initialization packet
/// (`cid`, `cmd | 0x80`, length) and continuation packets (`cid`, seq).
pub fn frame(
    cid: u32,
    cmd: u8,
    payload: &[u8],
    report_len: usize,
) -> Result<Vec<Vec<u8>>, HidError> {
    if report_len < 8 {
        return Err(HidError::Protocol("report too short"));
    }
    let len = u16::try_from(payload.len()).map_err(|_| HidError::TooLong)?;
    let first = report_len.saturating_sub(7);
    let rest = report_len.saturating_sub(5);
    let continuation = payload.len().saturating_sub(first).div_ceil(rest);
    if continuation > 128 {
        return Err(HidError::TooLong);
    }
    let mut reports = Vec::with_capacity(continuation.saturating_add(1));
    let (head, mut tail) = payload.split_at(first.min(payload.len()));
    let mut report = Vec::with_capacity(report_len);
    report.extend_from_slice(&cid.to_be_bytes());
    report.push(cmd | 0x80);
    report.extend_from_slice(&len.to_be_bytes());
    report.extend_from_slice(head);
    report.resize(report_len, 0);
    reports.push(report);
    let mut seq: u8 = 0;
    while !tail.is_empty() {
        let (chunk, next) = tail.split_at(rest.min(tail.len()));
        let mut report = Vec::with_capacity(report_len);
        report.extend_from_slice(&cid.to_be_bytes());
        report.push(seq);
        report.extend_from_slice(chunk);
        report.resize(report_len, 0);
        reports.push(report);
        seq = seq.saturating_add(1);
        tail = next;
    }
    Ok(reports)
}

/// One message being put back together from reports.
#[derive(Default)]
pub(crate) struct Assembly {
    cmd: u8,
    total: usize,
    data: Vec<u8>,
    next_seq: u8,
    started: bool,
}

impl Assembly {
    /// Feed one report for `cid`; `Some((cmd, payload))` once complete.
    pub(crate) fn feed(
        &mut self,
        cid: u32,
        report: &[u8],
    ) -> Result<Option<(u8, Vec<u8>)>, HidError> {
        let Some((head, body)) = report.split_first_chunk::<5>() else {
            return Ok(None);
        };
        let [c0, c1, c2, c3, kind] = *head;
        if u32::from_be_bytes([c0, c1, c2, c3]) != cid {
            // Another client's channel: not ours to read.
            return Ok(None);
        }
        if kind & 0x80 != 0 {
            let Some((len, data)) = body.split_first_chunk::<2>() else {
                return Err(HidError::Protocol("short init packet"));
            };
            self.cmd = kind & 0x7F;
            self.total = usize::from(u16::from_be_bytes(*len));
            self.data.clear();
            self.data
                .extend_from_slice(data.get(..self.total.min(data.len())).unwrap_or_default());
            self.next_seq = 0;
            self.started = true;
        } else {
            if !self.started || kind != self.next_seq {
                return Err(HidError::Protocol("unexpected continuation packet"));
            }
            self.next_seq = self.next_seq.saturating_add(1);
            let want = self.total.saturating_sub(self.data.len());
            self.data
                .extend_from_slice(body.get(..want.min(body.len())).unwrap_or_default());
        }
        if self.data.len() >= self.total {
            self.started = false;
            return Ok(Some((self.cmd, std::mem::take(&mut self.data))));
        }
        Ok(None)
    }
}

/// A CTAPHID channel on one device.
pub struct Channel {
    device: Box<dyn HidDevice>,
    cid: u32,
}

impl Channel {
    /// CTAPHID_INIT on the broadcast channel with a random nonce; the
    /// answer with the same nonce carries our channel id.
    pub fn open(
        mut device: Box<dyn HidDevice>,
        control: &mut Control<'_>,
    ) -> Result<Self, HidError> {
        let mut nonce = [0u8; 8];
        rand::rng().fill_bytes(&mut nonce);
        let report_len = device.report_len();
        for report in frame(BROADCAST_CID, CMD_INIT, &nonce, report_len)? {
            device.write_report(&report)?;
        }
        let mut assembly = Assembly::default();
        loop {
            check_time(control)?;
            let Some(report) = device.read_report(READ_SLICE)? else {
                continue;
            };
            let Some((cmd, payload)) = assembly.feed(BROADCAST_CID, &report)? else {
                continue;
            };
            if cmd != CMD_INIT {
                continue;
            }
            let Some((echo, rest)) = payload.split_first_chunk::<8>() else {
                return Err(HidError::Protocol("short init answer"));
            };
            if *echo != nonce {
                continue;
            }
            let Some(cid) = rest.first_chunk::<4>() else {
                return Err(HidError::Protocol("short init answer"));
            };
            let cid = u32::from_be_bytes(*cid);
            if cid == 0 || cid == BROADCAST_CID {
                return Err(HidError::Protocol("reserved channel id"));
            }
            return Ok(Self { device, cid });
        }
    }

    fn send(&mut self, cmd: u8, payload: &[u8]) -> Result<(), HidError> {
        for report in frame(self.cid, cmd, payload, self.device.report_len())? {
            self.device.write_report(&report)?;
        }
        Ok(())
    }

    /// Send `payload` as `cmd` and wait for the answer, passing keepalives
    /// on and cancelling when asked or past the deadline.
    pub fn transact(
        &mut self,
        cmd: u8,
        payload: &[u8],
        control: &mut Control<'_>,
    ) -> Result<Vec<u8>, HidError> {
        self.send(cmd, payload)?;
        let mut assembly = Assembly::default();
        // Set once CTAPHID_CANCEL went out: the reason and its grace end.
        let mut cancelling: Option<(HidError, Instant)> = None;
        loop {
            if cancelling.is_none() {
                let reason = if control.cancelled() {
                    Some(HidError::Cancelled)
                } else if Instant::now() >= control.deadline {
                    Some(HidError::Timeout)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    // The key answers the pending request with
                    // KEEPALIVE_CANCEL; wait briefly so the channel is
                    // clean, then give up either way.
                    let _ = self.send(CMD_CANCEL, &[]);
                    cancelling = Some((reason, Instant::now() + CANCEL_GRACE));
                }
            }
            if let Some((reason, until)) = &cancelling
                && Instant::now() >= *until
            {
                return Err(reason.clone());
            }
            let Some(report) = self.device.read_report(READ_SLICE)? else {
                continue;
            };
            let Some((answer, data)) = assembly.feed(self.cid, &report)? else {
                continue;
            };
            match answer {
                CMD_KEEPALIVE => {
                    if cancelling.is_none() {
                        (control.on_keepalive)(data.first().copied().unwrap_or_default());
                    }
                }
                CMD_ERROR => {
                    if let Some((reason, _)) = cancelling {
                        return Err(reason);
                    }
                    return Err(HidError::Device(data.first().copied().unwrap_or_default()));
                }
                answer if answer == cmd => {
                    if let Some((reason, _)) = cancelling {
                        return Err(reason);
                    }
                    return Ok(data);
                }
                _ => return Err(HidError::Protocol("unexpected answer")),
            }
        }
    }

    /// A CTAP2 CBOR command: the command byte and its CBOR parameters;
    /// the answer is a status byte and a CBOR map.
    pub fn cbor(
        &mut self,
        command: u8,
        params: Option<ciborium::Value>,
        control: &mut Control<'_>,
    ) -> Result<ciborium::Value, CtapError> {
        let mut payload = vec![command];
        if let Some(params) = params {
            ciborium::into_writer(&params, &mut payload)
                .map_err(|_| CtapError::Malformed("request"))?;
        }
        let answer = self.transact(CMD_CBOR, &payload, control)?;
        let Some((&code, body)) = answer.split_first() else {
            return Err(CtapError::Malformed("empty answer"));
        };
        if code != status::OK {
            return Err(CtapError::Status(code));
        }
        if body.is_empty() {
            return Ok(ciborium::Value::Map(Vec::new()));
        }
        ciborium::from_reader(body).map_err(|_| CtapError::Malformed("cbor"))
    }
}

fn check_time(control: &Control<'_>) -> Result<(), HidError> {
    if control.cancelled() {
        Err(HidError::Cancelled)
    } else if Instant::now() >= control.deadline {
        Err(HidError::Timeout)
    } else {
        Ok(())
    }
}
