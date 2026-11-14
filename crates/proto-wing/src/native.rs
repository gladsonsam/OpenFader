//! Native node definitions carried in OSC blobs. See WING Remote Protocols
//! V3.0.5-1 pp.25–26 and pp.84–85. These payloads are not TCP channel frames:
//! length-delimited field bytes are consumed literally, without DF escaping.
use anyhow::{ensure, Context};

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub parent: i32,
    pub hash: i32,
    pub index: u16,
    pub name: String,
    pub label: String,
    pub unit: u8,
    pub read_only: bool,
    pub kind: Kind,
}
impl Definition {
    /// OSC child address: named leaves use their native name; indexed nodes
    /// use the documented 1-based index, never the display label.
    pub fn address_name(&self) -> anyhow::Result<String> {
        let name = if self.name.is_empty() {
            anyhow::ensure!(self.index > 0, "WING: unnamed native child has zero index");
            self.index.to_string()
        } else {
            self.name.clone()
        };
        anyhow::ensure!(
            !name.is_empty()
                && name != "."
                && name != ".."
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'$')),
            "WING: unaddressable native child {name:?}"
        );
        Ok(name)
    }
}

/// Parse definitions and fail closed when native children cannot be uniquely
/// addressed, including collisions between a named and indexed child.
pub fn addressed_definitions(bytes: &[u8]) -> anyhow::Result<Vec<Definition>> {
    let defs = definitions(bytes)?;
    let mut seen = std::collections::BTreeSet::new();
    for def in &defs {
        let name = def.address_name()?;
        anyhow::ensure!(
            seen.insert(name.clone()),
            "WING: duplicate or ambiguous native child {name:?}"
        );
    }
    Ok(defs)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Node,
    Linear { min: f32, max: f32, steps: i32 },
    Log { min: f32, max: f32, steps: i32 },
    Fader,
    Integer { min: i32, max: i32 },
    Enum(Vec<(String, String)>),
    FloatEnum(Vec<(f32, String)>),
    Text { max_bytes: u16 },
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        ensure!(n <= self.0.len(), "WING: truncated native definition");
        let (value, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(value)
    }
    fn byte(&mut self) -> anyhow::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> anyhow::Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn long(&mut self) -> anyhow::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn signed(&mut self) -> anyhow::Result<i32> {
        Ok(self.long()? as i32)
    }
    fn float(&mut self) -> anyhow::Result<f32> {
        let value = f32::from_bits(self.long()?);
        ensure!(
            value.is_finite(),
            "WING: non-finite native definition value"
        );
        Ok(value)
    }
    fn text(&mut self) -> anyhow::Result<String> {
        let len = self.byte()? as usize;
        Ok(std::str::from_utf8(self.take(len)?)
            .context("WING: invalid native definition text")?
            .to_owned())
    }
}

/// Decode a complete DD reply, including its terminal DE. Reject malformed
/// or unsupported records rather than advertising guessed editing domains.
pub fn definitions(bytes: &[u8]) -> anyhow::Result<Vec<Definition>> {
    let mut input = Cursor(bytes);
    let mut out = Vec::new();
    loop {
        match input.byte()? {
            0xde => {
                ensure!(
                    input.0.is_empty(),
                    "WING: data after native definition terminator"
                );
                return Ok(out);
            }
            0xdf => {
                let short_len = input.word()?;
                let len = if short_len == 0 {
                    input.long()? as usize
                } else {
                    short_len as usize
                };
                let mut body = Cursor(input.take(len)?);
                let parent = body.signed()?;
                let hash = body.signed()?;
                let index = body.word()?;
                let name = body.text()?;
                let label = body.text()?;
                let flags = body.word()?;
                let kind = match (flags >> 4) & 15 {
                    0 => Kind::Node,
                    ty @ (1 | 2) => {
                        let min = body.float()?;
                        let max = body.float()?;
                        let steps = body.signed()?;
                        if ty == 1 {
                            Kind::Linear { min, max, steps }
                        } else {
                            Kind::Log { min, max, steps }
                        }
                    }
                    3 => {
                        // Some definitions include the fader's standard numeric
                        // domain, although the protocol also permits no payload.
                        if !body.0.is_empty() {
                            let min = body.float()?;
                            let max = body.float()?;
                            let steps = body.signed()?;
                            ensure!(
                                min == -144.0 && max == 10.0 && steps == 1024,
                                "WING: unsupported native fader definition"
                            );
                        }
                        Kind::Fader
                    }
                    4 => Kind::Integer {
                        min: body.signed()?,
                        max: body.signed()?,
                    },
                    5 => {
                        let count = body.word()? as usize;
                        // Each item needs at least its two length bytes.
                        ensure!(count <= body.0.len() / 2, "WING: truncated native enum");
                        let mut choices = Vec::with_capacity(count);
                        for _ in 0..count {
                            choices.push((body.text()?, body.text()?));
                        }
                        Kind::Enum(choices)
                    }
                    6 => {
                        let count = body.word()? as usize;
                        ensure!(
                            count <= body.0.len() / 5,
                            "WING: truncated native float enum"
                        );
                        let mut choices = Vec::with_capacity(count);
                        for _ in 0..count {
                            choices.push((body.float()?, body.text()?));
                        }
                        Kind::FloatEnum(choices)
                    }
                    7 => Kind::Text {
                        max_bytes: body.word()?,
                    },
                    _ => anyhow::bail!("WING: unknown native definition type"),
                };
                ensure!(body.0.is_empty(), "WING: trailing native definition fields");
                ensure!(out.len() < 4096, "WING: too many native definitions");
                out.push(Definition {
                    parent,
                    hash,
                    index,
                    name,
                    label,
                    unit: (flags & 15) as u8,
                    read_only: flags & 0x200 != 0,
                    kind,
                });
            }
            _ => anyhow::bail!("WING: unexpected native definition token"),
        }
    }
}

use crate::{WingDriver, QUERY_TIMEOUT};
use rosc::OscType;
use std::time::Instant;
impl WingDriver {
    pub async fn parameter_definition(&self, path: &str) -> anyhow::Result<Definition> {
        let (parent, name) = path
            .rsplit_once('/')
            .context("WING: invalid parameter path")?;
        ensure!(
            path.starts_with('/') && !parent.is_empty() && !name.is_empty(),
            "WING: invalid parameter path"
        );
        let session = self.session()?;
        let response = session
            .query_with_args_timeout(
                parent,
                vec![OscType::Blob(vec![0xdd])],
                QUERY_TIMEOUT,
                |msg| matches!(msg.args.as_slice(), [OscType::Blob(_)]),
            )
            .await?;
        self.ensure_current(&session)?;
        let [OscType::Blob(bytes)] = response.args.as_slice() else {
            unreachable!()
        };
        let mut found = addressed_definitions(bytes)?
            .into_iter()
            .filter(|def| def.address_name().is_ok_and(|child| child == name));
        let definition = found
            .next()
            .context("WING: native parameter definition unavailable")?;
        ensure!(
            found.next().is_none(),
            "WING: duplicate native parameter definition"
        );
        *self.last_control.lock().unwrap() = Some(Instant::now());
        Ok(definition)
    }
}
