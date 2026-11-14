//! WING group membership lives in comma-separated tags. #D1..#D16 is in
//! the protocol manual; #M1..#M8 is corroborated by recorded hardware
//! observations linked in README.md. OpenFader itself is not hardware-verified.
//! Unrelated tag tokens, whitespace and Unicode text are preserved verbatim.
use anyhow::ensure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Dca,
    Mute,
}
impl GroupKind {
    fn token(self, group: u8) -> anyhow::Result<String> {
        let (prefix, count) = match self {
            Self::Dca => ("D", 16),
            Self::Mute => ("M", 8),
        };
        ensure!(
            (1..=count).contains(&group),
            "WING: {prefix} group out of range"
        );
        Ok(format!("#{prefix}{group}"))
    }
}

pub fn member(tags: &str, kind: GroupKind, group: u8) -> anyhow::Result<bool> {
    let wanted = kind.token(group)?;
    Ok(tags.split(',').any(|tag| tag.trim() == wanted))
}
pub fn edit(tags: &str, kind: GroupKind, group: u8, member: bool) -> anyhow::Result<String> {
    let wanted = kind.token(group)?;
    if self::member(tags, kind, group)? == member {
        return Ok(tags.to_owned());
    }
    let out = if member {
        let mut out = tags.to_owned();
        if !out.is_empty() && !out.trim_end().ends_with(',') {
            out.push(',');
        }
        out.push_str(&wanted);
        out
    } else {
        tags.split(',')
            .filter(|tag| tag.trim() != wanted)
            .collect::<Vec<_>>()
            .join(",")
    };
    ensure!(
        out.len() <= 80,
        "WING: assignment would exceed the 80-byte tag limit"
    );
    Ok(out)
}
/// A console may reorder membership tokens while retaining the same groups.
/// User tags remain byte-for-byte and in their original relative order.
pub fn equivalent(expected: &str, actual: &str) -> bool {
    let split = |tags: &str| {
        let mut dca = [0u16; 16];
        let mut mute = [0u16; 8];
        let mut other = Vec::new();
        for token in tags.split(',') {
            let trimmed = token.trim();
            let index = |prefix: &str, limit: usize| {
                let suffix = trimmed.strip_prefix(prefix)?;
                let number: usize = suffix.parse().ok()?;
                (number >= 1 && number <= limit && suffix == number.to_string())
                    .then_some(number - 1)
            };
            if let Some(i) = index("#D", 16) {
                dca[i] = dca[i].saturating_add(1);
            } else if let Some(i) = index("#M", 8) {
                mute[i] = mute[i].saturating_add(1);
            } else {
                other.push(token.to_string());
            }
        }
        (dca, mute, other)
    };
    split(expected) == split(actual)
}

// Preserve the original public DCA helper API.
pub fn assigned(tags: &str, group: u8) -> anyhow::Result<bool> {
    member(tags, GroupKind::Dca, group)
}
pub fn set_assignment(tags: &str, group: u8, value: bool) -> anyhow::Result<String> {
    edit(tags, GroupKind::Dca, group, value)
}

use crate::mapping::{root, Mapping, WireKind};
use openfader_core::StripId;
pub(crate) fn group_assignment(param: &str) -> Option<(GroupKind, u8)> {
    if let Some(group) = param.strip_prefix("_mg").and_then(|g| g.parse::<u8>().ok()) {
        return (1..=8).contains(&group).then_some((GroupKind::Mute, group));
    }
    let group: u8 = param
        .strip_prefix("dca")?
        .strip_suffix("_assign")?
        .parse()
        .ok()?;
    (1..=16).contains(&group).then_some((GroupKind::Dca, group))
}
pub(crate) fn tag_mapping(strip: StripId) -> anyhow::Result<Mapping> {
    let (root, kind) = root(strip)?;
    ensure!(
        kind != openfader_core::StripKind::Dca,
        "WING: DCA strips have no group membership tags"
    );
    Ok(Mapping {
        path: format!("{root}/tags"),
        kind: WireKind::Text(80),
        model: None,
    })
}
