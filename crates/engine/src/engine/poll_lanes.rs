//! Poll lanes operations.

use super::*;

impl Engine {
    pub(super) fn queue_strip_repair(&self, ch: u16) {
        let driver = self.active_driver();
        let list = self.static_poll_list(&driver);
        let mut queue = self.focus_queue.write().unwrap();
        for (strip, id) in list.into_iter().filter(|(strip, _)| *strip == ch) {
            if !queue.iter().any(|(_, s, p, _)| *s == strip && p == id) {
                queue.push_back((driver.clone(), strip, id.to_string(), String::new()));
            }
        }
    }

    // ================= R2: pipelined poller =================

    /// Visible-group lane: (strip, param) pairs serving the currently
    /// visible strips' EQ/dyn/sends first (R2 priority lane between focus
    /// and background). One full sweep per focus (then disarms, see
    /// `prio_armed`); `k` caps the take; the cursor round-robins within
    /// the sweep.
    pub(super) fn pop_visible_batch(&self, driver: &str, k: usize) -> Vec<(u16, String)> {
        if k == 0
            || (!self.drv(driver).prio_always_armed() && !self.prio_armed.load(Ordering::Relaxed))
        {
            return Vec::new();
        }
        let vis = self.visible.read().unwrap().clone();
        if vis.is_empty() {
            return Vec::new();
        }
        let list = self.visible_poll_list(driver, &vis);
        if list.is_empty() {
            return Vec::new();
        }
        let dropped = self.param_dropped.read().unwrap();
        let mut out = Vec::new();
        let mut skips = 0usize;
        while out.len() < k && skips <= list.len() {
            let i = self.prio_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
            // Sweep wrap (index 0 after serving ≥1 entry): the one-shot
            // boost is done, disarm and yield to the background.
            let taken = self.prio_taken.fetch_add(1, Ordering::Relaxed);
            if i == 0 && taken > 0 {
                self.prio_armed.store(false, Ordering::Relaxed);
                if !self.drv(driver).prio_always_armed() {
                    break;
                }
            }
            let (ch, id) = &list[i];
            if dropped.contains(&(driver.to_string(), id.to_string())) {
                skips += 1;
                continue;
            }
            skips = 0;
            out.push((*ch, id.to_string()));
        }
        out
    }

    /// Cached visible-group poll list: visible strips × driver caps
    /// filtered to EQ/dyn/sends ids + scopes. Rebuilt when the visible
    /// set or driver changes.
    pub(super) fn visible_poll_list(&self, driver: &str, vis: &[u16]) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_visible.read().unwrap();
            if let Some((d, v, list)) = cache.as_ref() {
                if d == driver && v == vis {
                    return list.clone();
                }
            }
        }
        let topo = self.driver_topology(driver);
        let caps = self.driver_capabilities(driver);
        let reg = self.registry(driver);
        let kind_of = |ch: u16| {
            topo.iter()
                .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                .map(|f| f.kind)
        };
        let mut out = Vec::new();
        for &ch in vis {
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in &caps {
                if !is_priority_id(id) {
                    continue;
                }
                let scoped = reg
                    .find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true);
                if !scoped {
                    continue;
                }
                out.push((ch, *id));
            }
        }
        *self.poll_visible.write().unwrap() = Some((driver.to_string(), vis.to_vec(), out.clone()));
        out
    }

    /// Cached essentials poll list: every strip × fader/mute/name/pan/
    /// color (caps + scopes filtered, like the static list) plus provider
    /// `solo` backstop ids (same rule as
    /// [`Engine::static_poll_list`]). ~150 pairs on a full-size desk (vs ~2.2k
    /// background): the always-fresh lane for the values every bank
    /// renders, so a reconnected app is desk-fresh in seconds without
    /// waiting out the background sweep. Built once per driver (topology ×
    /// caps × scopes never change at runtime); drop-class errors only SKIP
    /// entries at drain time.
    pub(super) fn core_poll_list(&self, driver: &str) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_core.read().unwrap();
            if let Some(v) = cache.get(driver) {
                return v.clone();
            }
        }
        let topo = self.driver_topology(driver);
        let caps = self.driver_capabilities(driver);
        let reg = self.registry(driver);
        let mut out = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                for id in &caps {
                    if !is_essentials_id(id) {
                        continue;
                    }
                    let scoped = reg
                        .find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    out.push((s as u16, *id));
                }
                // Solo rides the essentials lane as its own id (same rule
                // as the static list, see `static_poll_list`).
                for id in self.drv(driver).essentials_extra_ids(fam.kind) {
                    out.push((s as u16, id));
                }
            }
        }
        self.poll_core
            .write()
            .unwrap()
            .insert(driver.to_string(), out.clone());
        out
    }

    /// Next `k` essentials entries (round-robin; drop-class pairs skipped in
    /// place). The lane never disarms, it is the perpetual fast path for
    /// fader/mute/name/pan/color/solo, so the background reservation in
    /// [`Engine::poll_tick`] is what keeps the background sweep alive.
    pub(super) fn pop_core_batch(&self, driver: &str, k: usize) -> Vec<(u16, String)> {
        if k == 0 {
            return Vec::new();
        }
        let list = self.core_poll_list(driver);
        if list.is_empty() {
            return Vec::new();
        }
        let dropped = self.param_dropped.read().unwrap();
        let mut out = Vec::new();
        let mut skips = 0usize;
        while out.len() < k && skips <= list.len() {
            let i = self.core_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
            let (ch, id) = &list[i];
            if dropped.contains(&(driver.to_string(), id.to_string())) {
                skips += 1;
                continue;
            }
            skips = 0;
            out.push((*ch, id.to_string()));
        }
        out
    }
}

/// R2 visible-lane priority ids: EQ + dynamics + sends (+HPF). Everything
/// else drains on the background lane.
pub(super) fn is_priority_id(id: &str) -> bool {
    id == "eq_on"
        || id.starts_with("eq")
        || id.starts_with("gate_")
        || id.starts_with("comp_")
        || id.starts_with("send")
        || id.starts_with("aux")
        || id.starts_with("fx")
        || id.starts_with("hpf")
}

/// R2 focus core ids: the mixer strip essentials (`fader`+`mute` first
/// wave, `name`+`pan` second, see [`Engine::request_focus`]). `first`
/// selects the fader/mute wave; `false` the name/pan wave.
pub(super) fn is_core_id(id: &str, first: bool) -> bool {
    if first {
        id == "fader" || id == "mute"
    } else {
        id == "name" || id == "pan" || id == "balance"
    }
}

/// Safe recall order for full-plane scene params: input stage first
/// (gain → HPF), then gate → comp → EQ → sends, then placement/look.
/// Faders/mutes/names ride outside this order (fade block + name block).
pub(super) fn safe_order_key(id: &str) -> (u8, &str) {
    let lane = if [
        "gain", "trim", "phantom", "polarity", "delay_on", "delay_ms",
    ]
    .contains(&id)
    {
        2u8
    } else if id.starts_with("hpf") {
        3
    } else if id.starts_with("gate_") {
        4
    } else if id.starts_with("comp_") {
        5
    } else if id == "eq_on" || id.starts_with("eq") {
        6
    } else if id.starts_with("send") || id.starts_with("aux") || id.starts_with("fx") {
        7
    } else if id == "pan" || id == "balance" {
        8
    } else {
        9
    };
    (lane, id)
}
