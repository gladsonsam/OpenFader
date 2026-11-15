//! Stable capture provenance and provider-neutral playback mapping.
use crate::{ChannelLayout, TakeManifest};
use std::{collections::HashSet, io};

pub fn validate_capture_sources(sources: &[String], channels: u16) -> io::Result<()> {
    ChannelLayout::new(channels)?;
    if !sources.is_empty() && sources.len() != usize::from(channels) {
        return Err(io::Error::other(
            "capture source count differs from take channels",
        ));
    }
    if sources
        .iter()
        .any(|source| source.len() > 256 || source.chars().any(char::is_control))
    {
        return Err(io::Error::other("invalid capture source identity"));
    }
    Ok(())
}

pub fn take_capture_sources(manifest: &TakeManifest) -> io::Result<Vec<String>> {
    let sources = manifest
        .metadata
        .get("capture_sources")
        .map(|value| serde_json::from_value::<Vec<String>>(value.clone()))
        .transpose()
        .map_err(io::Error::other)?
        .unwrap_or_default();
    validate_capture_sources(&sources, manifest.channels)?;
    Ok(sources)
}

/// A user-confirmed import mapping is kept separate from capture provenance.
pub fn take_playback_sources(manifest: &TakeManifest) -> io::Result<Vec<String>> {
    if let Some(value) = manifest.metadata.get("playback_sources") {
        let sources: Vec<String> =
            serde_json::from_value(value.clone()).map_err(io::Error::other)?;
        validate_capture_sources(&sources, manifest.channels)?;
        Ok(sources)
    } else {
        take_capture_sources(manifest)
    }
}

pub fn save_playback_sources(dir: &std::path::Path, sources: &[String]) -> io::Result<()> {
    let mut manifest = crate::read_manifest(dir)?;
    if !manifest.complete {
        return Err(io::Error::other("take requires recovery"));
    }
    validate_capture_sources(sources, manifest.channels)?;
    manifest
        .metadata
        .insert("playback_sources".into(), serde_json::to_value(sources)?);
    crate::publish(dir, &manifest)
}

/// Resolve physical source identities rather than relying on track order or names.
/// Every output requested by the provider must have exactly one recorded source.
pub fn resolve_playback_mapping(
    recorded: &[String],
    expected: &[String],
    outputs: &[Option<usize>],
) -> io::Result<Vec<Option<usize>>> {
    if recorded.is_empty() {
        return Err(io::Error::other("This take has no saved capture mapping. Confirm its track mapping before automatic routing."));
    }
    if expected.len() != outputs.len() {
        return Err(io::Error::other("provider mapping length mismatch"));
    }
    let mut mapping = vec![None; recorded.len()];
    let mut used = HashSet::new();
    for (source, output) in expected.iter().zip(outputs) {
        let Some(output) = output else { continue };
        if source.is_empty() || !used.insert(*output) {
            return Err(io::Error::other("invalid provider playback mapping"));
        }
        let mut matches = recorded
            .iter()
            .enumerate()
            .filter(|(_, candidate)| *candidate == source);
        let Some((track, _)) = matches.next() else {
            return Err(io::Error::other(
                "This take does not cover the current mixer inputs. Review its track mapping.",
            ));
        };
        if matches.next().is_some() || mapping[track].is_some() {
            return Err(io::Error::other(
                "The capture mapping is ambiguous. Confirm the track assignments.",
            ));
        }
        mapping[track] = Some(*output);
    }
    if used.is_empty() {
        return Err(io::Error::other("no covered playback outputs"));
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn physical_sources_survive_reordered_and_partial_takes() {
        let expected = ids(&[
            "fixture:mic/1:raw",
            "fixture:mic/2:raw",
            "fixture:aux/1:raw",
        ]);
        assert_eq!(
            resolve_playback_mapping(
                &ids(&["fixture:mic/2:raw", "fixture:mic/1:raw"]),
                &expected,
                &[Some(4), Some(1), None]
            )
            .unwrap(),
            vec![Some(1), Some(4)]
        );
        assert!(resolve_playback_mapping(
            &ids(&["fixture:mic/2:raw"]),
            &expected,
            &[Some(4), Some(1), None]
        )
        .is_err());
        assert!(resolve_playback_mapping(
            &ids(&["fixture:mic/1:raw", "fixture:mic/1:raw"]),
            &expected,
            &[Some(0), None, None]
        )
        .is_err());
        assert!(resolve_playback_mapping(&[], &expected, &[Some(0), None, None]).is_err());
    }
    #[test]
    fn confirmed_mapping_preserves_original_capture_provenance() {
        let root = std::env::temp_dir().join(format!(
            "openfader-mapping-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = crate::TakeWriter::create_with_layout(
            &root,
            48_000,
            100,
            ChannelLayout::new(2).unwrap(),
        )
        .unwrap();
        writer
            .set_capture_sources(&ids(&["fixture:mic/1:raw", "fixture:mic/2:raw"]))
            .unwrap();
        writer.append_interleaved(&[0, 1, 2, 3]).unwrap();
        writer.finish().unwrap();
        let confirmed = ids(&["fixture:mic/2:raw", ""]);
        save_playback_sources(&root, &confirmed).unwrap();
        let manifest = crate::read_manifest(&root).unwrap();
        assert_eq!(
            take_capture_sources(&manifest).unwrap(),
            ids(&["fixture:mic/1:raw", "fixture:mic/2:raw"])
        );
        assert_eq!(take_playback_sources(&manifest).unwrap(), confirmed);
        assert!(save_playback_sources(&root, &ids(&["one-track"])).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_and_tap_identity_must_match() {
        assert!(resolve_playback_mapping(
            &ids(&["fixture:mic/1:post_eq"]),
            &ids(&["fixture:mic/1:raw"]),
            &[Some(0)]
        )
        .is_err());
        assert!(validate_capture_sources(&ids(&["fixture:1"]), 32).is_err());
        assert!(validate_capture_sources(&ids(&["bad\nsource"]), 1).is_err());
        for count in [2, 18, 32, 48, 64] {
            let sources: Vec<_> = (0..count).map(|n| format!("fixture:mic/{n}:raw")).collect();
            let outputs: Vec<_> = (0..count).map(Some).collect();
            assert_eq!(
                resolve_playback_mapping(&sources, &sources, &outputs).unwrap(),
                outputs
            );
        }
    }
}
