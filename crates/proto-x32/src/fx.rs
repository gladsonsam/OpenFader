//! FX rack: 8 slots, `/fx/N/type` (int), `/fx/N/par/NN` (effect-specific
//! float or integer), `/fx/N/source/l|r` (slots 1..4 only, int 0..17).
//!
//! Type tables from Maillot v4.09-02, appendix p166. The older independent
//! emulator reverses several entries (e.g. GEQ/TEQ2); production mappings
//! follow the documented firmware table, not those emulator discrepancies.
//! The documented slot domains are:
//! slots 1..4 select from 61 entries (`Sfxtyp1`, indices 0..60),
//! slots 5..8 from 34 entries (`Sfxtyp2`, indices 0..33).
//! Per-effect parameter counts, labels, wire types, ranges and options are
//! the documented Effects Parameters tables (pp88-118) behind
//! [`catalogue`]; [`wire`] maps normalized values onto the documented
//! float/int wire types. Typed reads/writes below are type-bound (the live
//! `/fx/N/type` selects the decode/encode row) and race-guarded (the type
//! is re-read after the write; a change mid-write fails instead of
//! mis-applying a value to an unrelated algorithm).
pub mod catalogue;
pub mod wire;

use crate::X32Driver;
use anyhow::ensure;

/// Slots 1..4: full catalogue (indices 0..60).
pub const FX_TYPES_1_4: &[&str] = &[
    "HALL", "AMBI", "RPLT", "ROOM", "CHAM", "PLAT", "VREV", "VRM", "GATE", "RVRS", "DLY", "3TAP",
    "4TAP", "CRS", "FLNG", "PHAS", "DIMC", "FILT", "ROTA", "PAN", "SUB", "D/RV", "CR/R", "FL/R",
    "D/CR", "D/FL", "MODD", "GEQ2", "GEQ", "TEQ2", "TEQ", "DES2", "DES", "P1A", "P1A2", "PQ5",
    "PQ5S", "WAVD", "LIM", "CMB", "CMB2", "FAC", "FAC1M", "FAC2", "LEC", "LEC2", "ULC", "ULC2",
    "ENH2", "ENH", "EXC2", "EXC", "IMG", "EDI", "SON", "AMP2", "AMP", "DRV2", "DRV", "PIT2", "PIT",
];

/// Slots 5..8: insertable effects only (indices 0..33).
pub const FX_TYPES_5_8: &[&str] = &[
    "GEQ2", "GEQ", "TEQ2", "TEQ", "DES2", "DES", "P1A", "P1A2", "PQ5", "PQ5S", "WAVD", "LIM",
    "FAC", "FAC1M", "FAC2", "LEC", "LEC2", "ULC", "ULC2", "ENH2", "ENH", "EXC2", "EXC", "IMG",
    "EDI", "SON", "AMP2", "AMP", "DRV2", "DRV", "PHAS", "FILT", "PAN", "SUB",
];

pub const FX_SLOTS: u8 = 8;
pub const FX_MAX_PAR: u8 = 64;

fn check_slot(slot: u8) -> anyhow::Result<u8> {
    ensure!((1..=FX_SLOTS).contains(&slot), "x32: FX slot out of range");
    Ok(slot)
}

fn check_par(param: u8) -> anyhow::Result<u8> {
    ensure!(
        (0..FX_MAX_PAR).contains(&param),
        "x32: FX param out of range"
    );
    Ok(param)
}

/// Type table for a slot (1-based).
pub fn fx_types(slot: u8) -> anyhow::Result<&'static [&'static str]> {
    check_slot(slot)?;
    Ok(if slot <= 4 {
        FX_TYPES_1_4
    } else {
        FX_TYPES_5_8
    })
}

pub fn fx_type_name(slot: u8, index: u8) -> Option<&'static str> {
    fx_types(slot).ok()?.get(index as usize).copied()
}

pub fn fx_type_index(slot: u8, name: &str) -> Option<u8> {
    fx_types(slot)
        .ok()?
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u8)
}

impl X32Driver {
    fn fx_type_addr(slot: u8) -> anyhow::Result<String> {
        check_slot(slot)?;
        Ok(format!("/fx/{slot}/type"))
    }

    fn fx_par_addr(slot: u8, param: u8) -> anyhow::Result<String> {
        check_slot(slot)?;
        check_par(param)?;
        Ok(format!("/fx/{slot}/par/{:02}", param + 1))
    }

    pub async fn get_fx_type_index(&self, slot: u8) -> anyhow::Result<u8> {
        let index = self.read_int(&Self::fx_type_addr(slot)?).await?;
        let table = fx_types(slot)?;
        ensure!(
            (0..table.len() as i32).contains(&index),
            "x32: FX type index out of range"
        );
        Ok(index as u8)
    }

    pub async fn set_fx_type_index(&self, slot: u8, index: u8) -> anyhow::Result<()> {
        let table = fx_types(slot)?;
        ensure!(
            (index as usize) < table.len(),
            "x32: FX type index out of range for slot {slot}"
        );
        self.send(&Self::fx_type_addr(slot)?, rosc::OscType::Int(index as i32))
            .await?;
        let actual = self.read_int(&Self::fx_type_addr(slot)?).await?;
        ensure!(
            actual == index as i32,
            "x32: FX type readback did not match the requested write"
        );
        Ok(())
    }

    pub async fn read_fx_par(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        self.read_fx_par_typed(slot, param, None).await
    }

    pub async fn write_fx_par(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        self.write_fx_par_checked(slot, param, value, None).await
    }

    /// Encoded single-param write for scene recall: sends the already
    /// validated `arg`, then re-reads the type leaf (a change mid-row fails
    /// with the expected code named) and the value leaf. Enum readback
    /// must land on the requested option index (half-step window);
    /// continuous FX params carry no documented desk steps (Effects
    /// Parameters pp88-118 list ranges only), so they agree within
    /// float roundoff — never a guessed quantum.
    pub async fn write_fx_par_encoded(
        &self,
        slot: u8,
        param: u8,
        code: &str,
        expected_index: u8,
        value: f32,
        arg: rosc::OscType,
    ) -> anyhow::Result<f32> {
        check_slot(slot)?;
        check_par(param)?;
        let addr = Self::fx_par_addr(slot, param)?;
        self.send(&addr, arg).await?;
        let live = self.get_fx_type_index(slot).await?;
        ensure!(
            live == expected_index,
            "x32: FX {slot} type changed during recall (expected {code})"
        );
        let actual = self.read_fx_par_typed(slot, param, Some(live)).await?;
        let eps = match catalogue::param_info(code, param + 1)
            .and_then(|info| info.options)
            .map(|options| options.len())
        {
            Some(n) if n >= 2 => 1.0 / (2 * (n - 1)) as f32 + 1e-6,
            // Continuous: no documented steps, float roundoff only.
            _ => crate::codec::WIRE_EPS,
        };
        ensure!(
            (actual - value).abs() <= eps,
            "x32: {code} par{:02} readback did not match the requested write",
            param + 1
        );
        Ok(actual)
    }

    /// Slot-aware canonical effect identity: the wire index is only
    /// meaningful with its slot domain (index 28 is GEQ on slots 1..4,
    /// DRV on slots 5..8); the global catalogue `code` is stable.
    pub fn fx_effect_code(slot: u8, index: u8) -> anyhow::Result<&'static str> {
        fx_type_name(slot, index).ok_or_else(|| {
            anyhow::anyhow!("x32: FX type index {index} out of range for slot {slot}")
        })
    }

    /// Documented parameter count for one catalogue effect.
    pub fn fx_effect_param_count(code: &str) -> Option<usize> {
        catalogue::param_count(code)
    }

    /// Typed single-query param read with an already-known effect index
    /// (connect burst / poller fast path). `None` (unknown/unreadable type)
    /// falls back to the two-query path via the live type leaf.
    pub async fn read_fx_par_typed(
        &self,
        slot: u8,
        param: u8,
        effect: Option<u8>,
    ) -> anyhow::Result<f32> {
        check_slot(slot)?;
        check_par(param)?;
        let index = match effect {
            Some(i) => i,
            None => self.get_fx_type_index(slot).await?,
        };
        let code = Self::fx_effect_code(slot, index)?;
        let addr = Self::fx_par_addr(slot, param)?;
        let session = self.session()?;
        let msg = session.query_timeout(&addr, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        ensure!(
            msg.args.len() == 1,
            "x32: FX parameter reply must carry exactly one argument"
        );
        let arg = msg.args.first().ok_or_else(|| {
            anyhow::anyhow!("x32: {code} par{:02} reply carries no argument", param + 1)
        })?;
        wire::decode_fx_par(code, param + 1, arg)
    }

    /// Type-bound, race-guarded param write: the live type selects the
    /// encode row, the write carries the documented wire type, and the type
    /// is re-read before the value readback. A type change mid-write (or a
    /// mismatch against `expected_effect`) fails instead of confirming a
    /// value against an unrelated algorithm. Out-of-count pars bail before
    /// any I/O.
    pub async fn write_fx_par_checked(
        &self,
        slot: u8,
        param: u8,
        value: f32,
        expected_effect: Option<u8>,
    ) -> anyhow::Result<()> {
        check_slot(slot)?;
        check_par(param)?;
        let index = self.get_fx_type_index(slot).await?;
        if let Some(expected) = expected_effect {
            ensure!(
                index == expected,
                "x32: FX {slot} type changed during write"
            );
        }
        let code = Self::fx_effect_code(slot, index)?;
        let arg = wire::encode_fx_par(code, param + 1, value)?;
        let addr = Self::fx_par_addr(slot, param)?;
        self.send(&addr, arg).await?;
        let index_after = self.get_fx_type_index(slot).await?;
        ensure!(
            index_after == index,
            "x32: FX {slot} type changed during write ({} -> {}); value uncertain",
            fx_type_name(slot, index).unwrap_or("?"),
            fx_type_name(slot, index_after).unwrap_or("?")
        );
        let actual = self.read_fx_par_typed(slot, param, Some(index)).await?;
        let steps = catalogue::param_info(code, param + 1)
            .and_then(|info| info.options)
            .map(|options| options.len());
        let eps = match steps {
            Some(n) if n >= 2 => 1.0 / (2 * (n - 1)) as f32 + 1e-6,
            // Continuous: no documented steps, float roundoff only.
            _ => crate::codec::WIRE_EPS,
        };
        ensure!(
            (actual - value).abs() <= eps,
            "x32: {code} par{:02} readback did not match the requested write",
            param + 1
        );
        Ok(())
    }

    pub async fn read_fx_source(&self, slot: u8, side: &str) -> anyhow::Result<u8> {
        check_slot(slot)?;
        ensure!(slot <= 4, "x32: FX source is slots 1..4 only");
        ensure!(side == "l" || side == "r", "x32: FX source side l|r");
        let index = self.read_int(&format!("/fx/{slot}/source/{side}")).await?;
        ensure!(
            (0..crate::addrs::FX_SOURCES.len() as i32).contains(&index),
            "x32: FX source out of range"
        );
        Ok(index as u8)
    }

    pub async fn write_fx_source(&self, slot: u8, side: &str, index: u8) -> anyhow::Result<()> {
        check_slot(slot)?;
        ensure!(slot <= 4, "x32: FX source is slots 1..4 only");
        ensure!(side == "l" || side == "r", "x32: FX source side l|r");
        ensure!(
            (index as usize) < crate::addrs::FX_SOURCES.len(),
            "x32: FX source out of range"
        );
        let path = format!("/fx/{slot}/source/{side}");
        self.send(&path, rosc::OscType::Int(index as i32)).await?;
        let actual = self.read_int(&path).await?;
        ensure!(
            actual == index as i32,
            "x32: FX source readback did not match the requested write"
        );
        Ok(())
    }
}
