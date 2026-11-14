//! Meter, gain-reduction, and tap sampling through provider contracts.

use super::*;

impl Engine {
    /// Current meter levels for the active driver (one entry per strip).
    pub(super) async fn meter_levels(&self) -> Vec<f32> {
        if self.reads_suspended.load(Ordering::Acquire) {
            return Vec::new();
        }
        self.active_drv().poll_meters().levels
    }

    /// Cached GR in positive dB reduction. Absence is not a received zero.
    /// Sim taps are its explicit synthetic GR source; hardware reads
    /// its DYN cache across topology inputs, buses and main.
    pub(super) async fn gr_db(&self, ch: usize) -> Option<(f32, f32)> {
        if self.reads_suspended.load(Ordering::Acquire) {
            return None;
        }
        let drv = self.active_drv();
        if let Some(taps) = drv.poll_taps() {
            let supported = drv.meter_taps();
            if !supported.contains(&"gr_gate") && !supported.contains(&"gr_comp") {
                return None;
            }
            return taps.get(ch).map(|t| (t.gr_gate_db, t.gr_comp_db));
        }
        if !drv.dyn_gr() {
            return None;
        }
        let id = StripId(u16::try_from(ch).ok()?);
        let read = |v| match v {
            Ok(ParamValue::Float(v)) if v.is_finite() => Some(v.max(0.0)),
            _ => None,
        };
        let gate = read(drv.get_param(id, "gate_gr").await);
        let comp = read(drv.get_param(id, "comp_gr").await);
        if gate.is_none() && comp.is_none() {
            return None;
        }
        // Bus/LR have no gate. The frontend checks per-strip parameter
        // support before exposing either component.
        Some((gate.unwrap_or(0.0), comp.unwrap_or(0.0)))
    }

    // ================= R2: meter taps =================

    /// Per-tap level vectors for the active driver, in
    /// `caps.meter_taps` order filtered to known taps. Drivers with a tap
    /// source (`poll_taps`) serve all their taps; otherwise only declared
    /// level taps are served from the meter blob cache (`post`, else `in`)
    /// and GR from the DYN cache. Drivers declaring no level tap (x32:
    /// GR only) emit GR frames without a level tap frame. Every
    /// entry is best-effort: a tap with no source reads back silence,
    /// never an error.
    pub(super) async fn tap_frames(&self) -> Vec<(String, Vec<f32>)> {
        if self.reads_suspended.load(Ordering::Acquire) {
            return Vec::new();
        }
        let drv = self.active_drv();
        if let Some(taps) = drv.poll_taps() {
            let supported = drv.meter_taps();
            return vec![
                ("in".to_string(), taps.iter().map(|t| t.input).collect()),
                ("post".to_string(), taps.iter().map(|t| t.post).collect()),
                (
                    "gr_gate".to_string(),
                    taps.iter().map(|t| t.gr_gate_db).collect(),
                ),
                (
                    "gr_comp".to_string(),
                    taps.iter().map(|t| t.gr_comp_db).collect(),
                ),
            ]
            .into_iter()
            .filter(|(tap, _)| supported.contains(&tap.as_str()))
            .collect();
        }
        let post = drv.poll_meters().levels;
        if drv.meter_taps().is_empty() {
            return Vec::new();
        }
        let n = self.driver_strips(&self.active_driver());
        let mut out = Vec::new();
        if !post.is_empty() {
            // `poll_meters` carries one level vector with no tap position
            // of its own: it serves the declared post tap, else the
            // declared pre tap — never an invented one. Drivers declaring
            // neither (x32: GR only) get no level tap; dl32s declares
            // only `in`.
            if drv.meter_taps().contains(&"post") {
                out.push(("post".to_string(), post));
            } else if drv.meter_taps().contains(&"in") {
                out.push(("in".to_string(), post));
            }
        }
        if drv.dyn_gr() {
            let mut gr_gate = vec![0.0f32; n];
            let mut gr_comp = vec![0.0f32; n];
            let mut available = false;
            for (i, (g, c)) in gr_gate.iter_mut().zip(gr_comp.iter_mut()).enumerate() {
                if let Some((gate, comp)) = self.gr_db(i).await {
                    available = true;
                    *g = gate;
                    *c = comp;
                }
            }
            if available {
                if drv.meter_taps().contains(&"gr_gate") {
                    out.push(("gr_gate".to_string(), gr_gate));
                }
                if drv.meter_taps().contains(&"gr_comp") {
                    out.push(("gr_comp".to_string(), gr_comp));
                }
            }
        }
        out
    }
}
