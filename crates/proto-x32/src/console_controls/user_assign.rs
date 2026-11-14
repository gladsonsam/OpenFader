//! X32 assignable controls, protocol 4.09-02 pp.22–23.
use super::specs::{ConsoleControl, ConsoleControlKind};

const COLORS: &[&str] = &[
    "OFF", "RD", "GN", "YE", "BL", "MG", "CY", "WH", "OFFi", "RDi", "GNi", "YEi", "BLi", "MGi",
    "CYi", "WHi",
];

#[rustfmt::skip]
pub(super) const CONTROLS: &[ConsoleControl] = &[
    ConsoleControl { id: "assign.a.color", category: "User assignments", label: "Set A color", path: "/config/userctrl/A/color", kind: ConsoleControlKind::Choice(COLORS) },
    ConsoleControl { id: "assign.a.encoder.1", category: "User assignments", label: "Encoder 1", path: "/config/userctrl/A/enc/1", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.encoder.2", category: "User assignments", label: "Encoder 2", path: "/config/userctrl/A/enc/2", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.encoder.3", category: "User assignments", label: "Encoder 3", path: "/config/userctrl/A/enc/3", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.encoder.4", category: "User assignments", label: "Encoder 4", path: "/config/userctrl/A/enc/4", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.5", category: "User assignments", label: "Button 5", path: "/config/userctrl/A/btn/5", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.6", category: "User assignments", label: "Button 6", path: "/config/userctrl/A/btn/6", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.7", category: "User assignments", label: "Button 7", path: "/config/userctrl/A/btn/7", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.8", category: "User assignments", label: "Button 8", path: "/config/userctrl/A/btn/8", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.9", category: "User assignments", label: "Button 9", path: "/config/userctrl/A/btn/9", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.10", category: "User assignments", label: "Button 10", path: "/config/userctrl/A/btn/10", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.11", category: "User assignments", label: "Button 11", path: "/config/userctrl/A/btn/11", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.a.button.12", category: "User assignments", label: "Button 12", path: "/config/userctrl/A/btn/12", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.color", category: "User assignments", label: "Set B color", path: "/config/userctrl/B/color", kind: ConsoleControlKind::Choice(COLORS) },
    ConsoleControl { id: "assign.b.encoder.1", category: "User assignments", label: "Encoder 1", path: "/config/userctrl/B/enc/1", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.encoder.2", category: "User assignments", label: "Encoder 2", path: "/config/userctrl/B/enc/2", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.encoder.3", category: "User assignments", label: "Encoder 3", path: "/config/userctrl/B/enc/3", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.encoder.4", category: "User assignments", label: "Encoder 4", path: "/config/userctrl/B/enc/4", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.5", category: "User assignments", label: "Button 5", path: "/config/userctrl/B/btn/5", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.6", category: "User assignments", label: "Button 6", path: "/config/userctrl/B/btn/6", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.7", category: "User assignments", label: "Button 7", path: "/config/userctrl/B/btn/7", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.8", category: "User assignments", label: "Button 8", path: "/config/userctrl/B/btn/8", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.9", category: "User assignments", label: "Button 9", path: "/config/userctrl/B/btn/9", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.10", category: "User assignments", label: "Button 10", path: "/config/userctrl/B/btn/10", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.11", category: "User assignments", label: "Button 11", path: "/config/userctrl/B/btn/11", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.b.button.12", category: "User assignments", label: "Button 12", path: "/config/userctrl/B/btn/12", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.color", category: "User assignments", label: "Set C color", path: "/config/userctrl/C/color", kind: ConsoleControlKind::Choice(COLORS) },
    ConsoleControl { id: "assign.c.encoder.1", category: "User assignments", label: "Encoder 1", path: "/config/userctrl/C/enc/1", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.encoder.2", category: "User assignments", label: "Encoder 2", path: "/config/userctrl/C/enc/2", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.encoder.3", category: "User assignments", label: "Encoder 3", path: "/config/userctrl/C/enc/3", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.encoder.4", category: "User assignments", label: "Encoder 4", path: "/config/userctrl/C/enc/4", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.5", category: "User assignments", label: "Button 5", path: "/config/userctrl/C/btn/5", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.6", category: "User assignments", label: "Button 6", path: "/config/userctrl/C/btn/6", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.7", category: "User assignments", label: "Button 7", path: "/config/userctrl/C/btn/7", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.8", category: "User assignments", label: "Button 8", path: "/config/userctrl/C/btn/8", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.9", category: "User assignments", label: "Button 9", path: "/config/userctrl/C/btn/9", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.10", category: "User assignments", label: "Button 10", path: "/config/userctrl/C/btn/10", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.11", category: "User assignments", label: "Button 11", path: "/config/userctrl/C/btn/11", kind: ConsoleControlKind::UserAssignment(7) },
    ConsoleControl { id: "assign.c.button.12", category: "User assignments", label: "Button 12", path: "/config/userctrl/C/btn/12", kind: ConsoleControlKind::UserAssignment(7) },
];

/// Validated native assignment codes from protocol pp.117–122. Ambiguous page
/// jump syntax remains unsupported rather than accepting arbitrary desk text.
pub(super) fn validate(encoder: bool, text: &str) -> bool {
    let b = text.as_bytes();
    if b == b"-" {
        return true;
    }
    let num = |slice: &[u8]| -> Option<u32> {
        if slice.is_empty() || !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(slice).ok()?.parse().ok()
    };
    match b {
        [b'F' | b'P', a, c] if encoder => num(&[*a, *c]).is_some_and(|v| v <= 79),
        [b'S', a, c, d, e] if encoder => {
            num(&[*a, *c]).is_some_and(|v| v <= 79) && num(&[*d, *e]).is_some_and(|v| v <= 15)
        }
        [b'X', slot, a, c] => {
            (b'0'..=b'7').contains(slot) && num(&[*a, *c]).is_some_and(|v| v <= 63)
        }
        [b'M', kind, a, c, d, e, f] => {
            let kind_ok = if encoder {
                b"CNP".contains(kind)
            } else {
                b"CNPcn".contains(kind)
            };
            kind_ok
                && num(&[*a, *c]).is_some_and(|v| (1..=16).contains(&v))
                && num(&[*d, *e, *f]).is_some_and(|v| v <= 127)
        }
        [b'R', a, c, d] => num(&[*a, *c, *d]).is_some_and(|v| v <= if encoder { 8 } else { 34 }),
        [b'D', c] if encoder => (b'@'..=b'L').contains(c),
        [b'U', c] if encoder => (b'0'..=b'2').contains(c),
        [b'O', a, c] if !encoder => num(&[*a, *c]).is_some_and(|v| v <= 85),
        [b'I', a, c] if !encoder => num(&[*a, *c]).is_some_and(|v| v <= 71),
        [b'S', bank, a, c] if !encoder => num(&[*a, *c]).is_some_and(|v| match bank {
            b'9' => v <= 3,
            b'0' | b'2' | b'4' => v <= 99,
            _ => false,
        }),
        [b'T', c] if !encoder => c.is_ascii_digit(),
        [b'U', a, c] if !encoder => num(&[*a, *c]).is_some_and(|v| v <= 13),
        [b'L', a, c, d] if !encoder => num(&[*a, *c, *d]).is_some_and(|v| v <= 199),
        [b'A', c] if !encoder => b"01".contains(c),
        _ => false,
    }
}
