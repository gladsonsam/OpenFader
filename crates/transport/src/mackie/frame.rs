//! Frame encoding, decoding, checksums, and wire enums.

/// First header byte of every frame.
pub const HEADER_MARKER: u8 = 0xAB;
/// Bytes of the fixed frame header (marker/seq/chunks/type/command).
pub const HEADER_LEN: usize = 6;
/// Header + header checksum.
pub const FRAME_PREFIX_LEN: usize = 8;

/// Frame direction/kind on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Request = 0,
    Response = 1,
    Error = 5,
    Broadcast = 8,
}

impl TryFrom<u8> for MessageType {
    type Error = FrameError;

    fn try_from(v: u8) -> Result<Self, FrameError> {
        match v {
            0 => Ok(Self::Request),
            1 => Ok(Self::Response),
            5 => Ok(Self::Error),
            8 => Ok(Self::Broadcast),
            _ => Err(FrameError::UnknownType(v)),
        }
    }
}

/// Command byte. Snapshot/file commands come from wire observation; the OSC/
/// meter commands match the family-wide values both sources agree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    KeepAlive = 0x01,
    Reboot = 0x05,
    ClientHandshake = 0x03,
    FirmwareInfo = 0x04,
    ChannelInfoControl = 0x06,
    ShowSnapshot = 0x07,
    FileInfo = 0x09,
    FileOpen = 0x0A,
    FileClose = 0x0B,
    FileRead = 0x0C,
    PostFile = 0x0D,
    GeneralInfo = 0x0E,
    SetSystemState = 0x0F,
    ShowStatus = 0x12,
    ChannelValues = 0x13,
    BroadcastControl = 0x15,
    MeterLayout = 0x16,
    ChannelNames = 0x18,
}

impl TryFrom<u8> for Command {
    type Error = FrameError;

    fn try_from(v: u8) -> Result<Self, FrameError> {
        match v {
            0x01 => Ok(Self::KeepAlive),
            0x05 => Ok(Self::Reboot),
            0x03 => Ok(Self::ClientHandshake),
            0x04 => Ok(Self::FirmwareInfo),
            0x06 => Ok(Self::ChannelInfoControl),
            0x07 => Ok(Self::ShowSnapshot),
            0x09 => Ok(Self::FileInfo),
            0x0A => Ok(Self::FileOpen),
            0x0B => Ok(Self::FileClose),
            0x0C => Ok(Self::FileRead),
            0x0D => Ok(Self::PostFile),
            0x0E => Ok(Self::GeneralInfo),
            0x0F => Ok(Self::SetSystemState),
            0x12 => Ok(Self::ShowStatus),
            0x13 => Ok(Self::ChannelValues),
            0x15 => Ok(Self::BroadcastControl),
            0x16 => Ok(Self::MeterLayout),
            0x18 => Ok(Self::ChannelNames),
            _ => Err(FrameError::UnknownCommand(v)),
        }
    }
}

/// Frame-level decode/encode failures.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    BadMarker(u8),
    UnknownType(u8),
    UnknownCommand(u8),
    BadBodyLen,
    BadHeaderChecksum { want: u16, got: u16 },
    Truncated,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMarker(b) => write!(f, "bad header marker {b:#04x}"),
            Self::UnknownType(t) => write!(f, "unknown message type {t:#04x}"),
            Self::UnknownCommand(c) => write!(f, "unknown command {c:#04x}"),
            Self::BadBodyLen => write!(f, "body length not a multiple of 4"),
            Self::BadHeaderChecksum { want, got } => {
                write!(
                    f,
                    "header checksum mismatch want={want:#06x} got={got:#06x}"
                )
            }
            Self::Truncated => write!(f, "truncated frame"),
        }
    }
}

impl std::error::Error for FrameError {}

/// One decoded frame. `body` is in network order, length a multiple of 4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub seq: u8,
    pub msg_type: MessageType,
    pub command: Command,
    pub body: Vec<u8>,
}

impl Message {
    pub fn new(
        seq: u8,
        msg_type: MessageType,
        command: Command,
        body: Vec<u8>,
    ) -> Result<Self, FrameError> {
        if !body.len().is_multiple_of(4) {
            return Err(FrameError::BadBodyLen);
        }
        Ok(Self {
            seq,
            msg_type,
            command,
            body,
        })
    }

    pub fn chunk_count(&self) -> usize {
        self.body.len() / 4
    }

    /// `0xFFFF` minus the sum of the 6 header bytes.
    pub fn header_checksum(header: &[u8; HEADER_LEN]) -> u16 {
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        (0xFFFF - (sum & 0xFFFF)) as u16
    }

    /// `0xFFFFFFFF` minus the sum of the body bytes.
    pub fn body_checksum(body: &[u8]) -> u32 {
        let sum: u64 = body.iter().map(|b| *b as u64).sum();
        (0xFFFFFFFF - (sum & 0xFFFFFFFF)) as u32
    }

    pub fn encode(&self) -> Vec<u8> {
        debug_assert_eq!(self.body.len() % 4, 0);
        let mut header = [0u8; HEADER_LEN];
        header[0] = HEADER_MARKER;
        header[1] = self.seq;
        header[2..4].copy_from_slice(&(self.chunk_count() as u16).to_be_bytes());
        header[4] = self.msg_type as u8;
        header[5] = self.command as u8;
        let checksum = Self::header_checksum(&header);

        let mut out = Vec::with_capacity(FRAME_PREFIX_LEN + self.body.len() + 4);
        out.extend_from_slice(&header);
        out.extend_from_slice(&checksum.to_be_bytes());
        if self.body.is_empty() {
            return out;
        }
        out.extend_from_slice(&self.body);
        out.extend_from_slice(&Self::body_checksum(&self.body).to_be_bytes());
        out
    }
}

/// Decode one frame from the front of `buf`.
///
/// Returns the message plus the total bytes consumed (header + body +
/// checksum), or `Ok(None)` when `buf` does not hold a full frame yet.
/// The header checksum is validated; the body checksum is computed by
/// [`Message::body_checksum`] but not enforced, matching the reference
/// implementation (which ignores it like DigiMixer does).
pub fn decode_one(buf: &[u8]) -> Result<Option<(Message, usize)>, FrameError> {
    if buf.len() < FRAME_PREFIX_LEN {
        return Ok(None);
    }
    if buf[0] != HEADER_MARKER {
        return Err(FrameError::BadMarker(buf[0]));
    }
    let seq = buf[1];
    let chunks = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let msg_type = MessageType::try_from(buf[4])?;
    let command = Command::try_from(buf[5])?;
    let want = u16::from_be_bytes([buf[6], buf[7]]);
    let header: &[u8; HEADER_LEN] = buf[0..HEADER_LEN].try_into().expect("prefix checked");
    let got = Message::header_checksum(header);
    if want != got {
        return Err(FrameError::BadHeaderChecksum { want, got });
    }
    if chunks == 0 {
        let msg = Message {
            seq,
            msg_type,
            command,
            body: Vec::new(),
        };
        return Ok(Some((msg, FRAME_PREFIX_LEN)));
    }
    let total = FRAME_PREFIX_LEN + chunks * 4 + 4;
    if buf.len() < total {
        return Ok(None);
    }
    let body = buf[FRAME_PREFIX_LEN..FRAME_PREFIX_LEN + chunks * 4].to_vec();
    let msg = Message {
        seq,
        msg_type,
        command,
        body,
    };
    Ok(Some((msg, total)))
}

/// Reverse the bytes inside each 4-byte word.
///
/// The mixer stores text as little-endian characters inside an otherwise
/// big-endian protocol, so `"MGZU"` travels as `55 5A 47 4D`. Applies to
/// channel names, snapshot names and file payloads. Trailing bytes past the
/// last whole word are dropped: the protocol always pads to a word.
pub fn word_swap(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() - data.len() % 4);
    for word in data.as_chunks::<4>().0 {
        out.extend_from_slice(&[word[3], word[2], word[1], word[0]]);
    }
    out
}
