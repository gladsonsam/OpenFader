//! Generic-send destinations: 16 mix buses plus 6 matrices on distinct
//! canonical slots.
//!
//! The engine builds its `SendDestination` rows from
//! [`send_destination`]; this crate stays dependency-free of the engine.
//! Slots are 1-based, matching canonical `sendN` ids. Because one global
//! slot identity cannot mean bus and matrix depending on the source,
//! matrix destinations live on slots 17..22 (`send17` → matrix 1, …):
//! input strips serve 1..16 (buses) while bus/main strips serve 17..22
//! (matrices); anything else is `None` (honestly unavailable).

/// Send slot → `(key, label)`. Slots 1..=16 address bus 1..16, slots
/// 17..=22 address matrix 1..6; anything else is `None`.
pub fn send_destination(slot: u16) -> Option<(String, String)> {
    if (1..=16).contains(&slot) {
        return Some((format!("bus/{slot:02}"), format!("Bus {slot}")));
    }
    if (17..=22).contains(&slot) {
        let mtx = slot - 16;
        return Some((format!("mtx/{mtx:02}"), format!("Mtx {mtx}")));
    }
    None
}
