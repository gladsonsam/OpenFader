//! Explicit mappings for providers that implement the canonical aux/FX sends.
//! Consumers use destination identity rather than guessing from driver names.
use super::SendDestination;

pub(super) fn legacy_send_destination(
    slot: u16,
    auxes: u16,
    effects: u16,
) -> Option<SendDestination> {
    let (family, label, index) = match slot {
        0 => return None,
        n if n <= auxes => ("bus", "Aux", n),
        n if n <= auxes + effects => ("fx", "FX", n - auxes),
        _ => return None,
    };
    Some(SendDestination {
        key: format!("{family}/{index:02}"),
        label: format!("{label} {index}"),
        stereo: false,
        allow_self: true,
    })
}
