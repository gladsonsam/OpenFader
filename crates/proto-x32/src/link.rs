//! Stereo links: `/config/chlink/1-2`, `auxlink`, `fxlink`,
//! `buslink`, `mtxlink` (`1` = linked). 35 pairs: ch 16, aux 4, fxrtn
//! 4, bus 8, mtx 3. Main/DCA have no link leaf.
//!
//! A linked pair shares the desk control surface; linked members are
//! reported together so the engine authorizes every affected strip
//! before any write. An unreadable link state rejects the operation.
use crate::{mapping::fam_of, X32Driver};
use openfader_core::StripId;

/// All linked-pair members, in strip-index order.
pub const STEREO_PAIRS: &[[u16; 2]] = &{
    let mut pairs = [[0u16; 2]; 35];
    let mut n = 0;
    // ch 1-2 .. 31-32 (strips 0..32)
    let mut left = 0u16;
    while left < 32 {
        pairs[n] = [left, left + 1];
        n += 1;
        left += 2;
    }
    // auxin 1-2 .. 7-8 (strips 32..40)
    left = 32;
    while left < 40 {
        pairs[n] = [left, left + 1];
        n += 1;
        left += 2;
    }
    // fxrtn 1-2 .. 7-8 (strips 40..48)
    left = 40;
    while left < 48 {
        pairs[n] = [left, left + 1];
        n += 1;
        left += 2;
    }
    // bus 1-2 .. 15-16 (strips 48..64)
    left = 48;
    while left < 64 {
        pairs[n] = [left, left + 1];
        n += 1;
        left += 2;
    }
    // mtx 1-2 .. 5-6 (strips 64..70)
    left = 64;
    while left < 70 {
        pairs[n] = [left, left + 1];
        n += 1;
        left += 2;
    }
    pairs
};

/// Link leaf + pair base for a strip, or `None` when the family has no
/// link leaf (main, DCA).
fn link_leaf(strip: StripId) -> anyhow::Result<Option<(String, StripId)>> {
    let i = strip.0 as usize;
    let fam = crate::addrs::X32_TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .ok_or_else(|| anyhow::anyhow!("x32: strip out of range"))?;
    let (leaf, base) = match fam.key {
        "ch" => ("chlink", 0),
        "auxin" => ("auxlink", 32),
        "fxrtn" => ("fxlink", 40),
        "bus" => ("buslink", 48),
        "mtx" => ("mtxlink", 64),
        _ => return Ok(None),
    };
    let local = i - base;
    let pair = local / 2;
    let first_wire = pair * 2 + 1;
    Ok(Some((
        format!("/config/{leaf}/{}-{}", first_wire, first_wire + 1),
        StripId((base + pair * 2) as u16),
    )))
}

impl X32Driver {
    /// Members affected by a shared linked control: both pair members
    /// while the desk reports linked, else the strip alone. Unreadable
    /// link state rejects the operation because the partner may be affected.
    pub async fn linked_members(&self, strip: StripId) -> anyhow::Result<Vec<StripId>> {
        let Some((path, base)) = link_leaf(strip)? else {
            return Ok(vec![strip]);
        };
        let linked = self.read_int(&path).await?;
        anyhow::ensure!((0..=1).contains(&linked), "x32: invalid stereo link state");
        if linked != 0 {
            Ok(vec![base, StripId(base.0 + 1)])
        } else {
            Ok(vec![strip])
        }
    }

    /// Set the pair link and confirm it on the same live session.
    pub async fn write_stereo_link(
        &self,
        strip: StripId,
        linked: bool,
    ) -> anyhow::Result<Vec<(StripId, String, openfader_core::ParamValue)>> {
        let _ = fam_of(strip)?;
        let Some((path, _base)) = link_leaf(strip)? else {
            anyhow::bail!("x32: stereo linking unavailable on this strip")
        };
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session
            .send(&path, vec![rosc::OscType::Int(i32::from(linked))])
            .await?;
        // Fresh independent readback of the same leaf.
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let actual = crate::mapping::Mapping {
            path,
            kind: crate::mapping::WireKind::Int(0, 1),
        }
        .decode(&reply)?;
        anyhow::ensure!(
            actual == openfader_core::ParamValue::Float(f32::from(linked)),
            "x32: link readback did not match the requested write"
        );
        Ok(vec![])
    }
}
