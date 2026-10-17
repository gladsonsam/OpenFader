//! Scenes and shows: named, recallable mixer states with fades.
//!
//! A [`Scene`] is a self-contained snapshot: per-strip fader/mute plus an
//! open-ended params map and per-driver notes namespaced as
//! `"<driver>:<key>"` (e.g. `"xair:fxrack"`, `"dl32s:mutegroups"`).
//! A [`Show`] is an ordered library of scenes (soundcheck / set-1 / sermon …).
//!
//! Recall ramps faders from their live values to the scene targets over
//! `fade_ms` ([`Scene::recall_to`]). Callers wanting fire-and-forget can
//! `tokio::spawn` the future; [`MixerDriver::recall_scene`] is the same
//! operation as a default trait method.

use std::collections::HashMap;
use std::time::Duration;

use crate::{clamp_fader, MixerDriver, MixerSnapshot, ParamValue, StripId};

/// Fade used by convenience wrappers (ms).
pub const DEFAULT_FADE_MS: u64 = 400;
/// Ramp steps per fade.
pub const DEFAULT_FADE_STEPS: usize = 20;

/// One strip's contribution to a scene.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneStrip {
    pub strip: StripId,
    pub name: String,
    pub fader: f32,
    pub mute: bool,
    /// Extra params (`"pan"`, driver-specific ids…) applied after faders.
    pub params: HashMap<String, ParamValue>,
}

impl SceneStrip {
    pub fn new(strip: StripId, name: impl Into<String>, fader: f32, mute: bool) -> Self {
        Self {
            strip,
            name: name.into(),
            fader,
            mute,
            params: HashMap::new(),
        }
    }
}

/// A named, recallable scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub name: String,
    pub strips: Vec<SceneStrip>,
    pub main_fader: Option<f32>,
    /// Cross-mixer notes, keyed `"<driver>:<key>"`. Use the helpers.
    pub notes: HashMap<String, String>,
}

impl Scene {
    /// Capture a scene from a live snapshot. Params maps start empty;
    /// fill them (or [`Scene::set_note`]) for driver-specific extras.
    pub fn capture(name: impl Into<String>, snapshot: &MixerSnapshot) -> Self {
        let strips = snapshot
            .strips
            .iter()
            .map(|s| SceneStrip::new(s.id, s.name.clone(), s.fader, s.mute))
            .collect();
        Self {
            name: name.into(),
            strips,
            main_fader: None,
            notes: HashMap::new(),
        }
    }

    pub fn strip(&self, id: StripId) -> Option<&SceneStrip> {
        self.strips.iter().find(|s| s.strip == id)
    }

    /// Namespaced note key: `"<driver>:<key>"`.
    pub fn note_key(driver: &str, key: &str) -> String {
        format!("{driver}:{key}")
    }

    /// Store a driver-namespaced note (FX-rack dumps, mute-group maps…).
    pub fn set_note(&mut self, driver: &str, key: &str, value: impl Into<String>) {
        self.notes
            .insert(Self::note_key(driver, key), value.into());
    }

    /// Fetch a driver-namespaced note.
    pub fn note(&self, driver: &str, key: &str) -> Option<&str> {
        self.notes.get(&Self::note_key(driver, key)).map(String::as_str)
    }

    /// Recall this scene onto `driver`, ramping faders over `fade_ms`.
    ///
    /// Order: pre-unmute strips whose target is unmuted (so the fade is
    /// audible), ramp faders, snap to exact targets, apply target mutes,
    /// apply extra params, then the main fader. `fade_ms == 0` jumps.
    /// Drivers without `get_fader` jump straight to targets.
    pub async fn recall_to<D: MixerDriver + Sync + ?Sized>(
        &self,
        driver: &D,
        fade_ms: u64,
    ) -> anyhow::Result<()> {
        for s in &self.strips {
            if !s.mute {
                driver.set_mute(s.strip, false).await?;
            }
        }
        if fade_ms == 0 {
            for s in &self.strips {
                driver.set_fader(s.strip, s.fader).await?;
            }
        } else {
            let mut starts = Vec::with_capacity(self.strips.len());
            for s in &self.strips {
                // Unsupported `get_fader` -> start at target (jump that strip).
                starts.push(driver.get_fader(s.strip).await.unwrap_or(s.fader));
            }
            let steps = DEFAULT_FADE_STEPS;
            let step_ms = fade_ms / steps as u64;
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                for (s, start) in self.strips.iter().zip(starts.iter()) {
                    driver
                        .set_fader(s.strip, clamp_fader(start + (s.fader - start) * t))
                        .await?;
                }
                if step < steps && step_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(step_ms)).await;
                }
            }
            // Snap to exact targets (no float residue).
            for s in &self.strips {
                driver.set_fader(s.strip, s.fader).await?;
            }
        }
        for s in &self.strips {
            if s.mute {
                driver.set_mute(s.strip, true).await?;
            }
        }
        for s in &self.strips {
            for (param, value) in &s.params {
                driver.set_param(s.strip, param, value.clone()).await?;
            }
        }
        if let Some(m) = self.main_fader {
            driver.set_main_fader(m).await?;
        }
        Ok(())
    }
}

/// Free-function form of [`Scene::recall_to`]; backs the default
/// [`MixerDriver::recall_scene`] method.
pub async fn recall_scene<D: MixerDriver + Sync + ?Sized>(
    driver: &D,
    scene: &Scene,
    fade_ms: u64,
) -> anyhow::Result<()> {
    scene.recall_to(driver, fade_ms).await
}

/// An ordered library of scenes (one per song / cue / service segment).
#[derive(Clone, Debug, Default)]
pub struct Show {
    pub name: String,
    pub scenes: Vec<Scene>,
}

impl Show {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scenes: Vec::new(),
        }
    }

    /// Add a scene, replacing any same-named scene (upsert keeps the
    /// library predictable for UI lists).
    pub fn add_scene(&mut self, scene: Scene) {
        if let Some(slot) = self.scenes.iter_mut().find(|s| s.name == scene.name) {
            *slot = scene;
        } else {
            self.scenes.push(scene);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.name == name)
    }

    /// Remove a scene by name; `true` when something was removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.scenes.len();
        self.scenes.retain(|s| s.name != name);
        self.scenes.len() != before
    }

    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.scenes.iter().map(|s| s.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_namespaced_per_driver() {
        let mut scene = Scene {
            name: "x".into(),
            strips: vec![],
            main_fader: None,
            notes: HashMap::new(),
        };
        scene.set_note("xair", "fxrack", "hall");
        scene.set_note("dl32s", "fxrack", "plate");
        assert_eq!(scene.note("xair", "fxrack"), Some("hall"));
        assert_eq!(scene.note("dl32s", "fxrack"), Some("plate"));
        assert_eq!(scene.note("xair", "missing"), None);
    }

    #[test]
    fn show_upsert_get_remove() {
        let mut show = Show::new("sunday");
        let mk = |name: &str| Scene {
            name: name.into(),
            strips: vec![],
            main_fader: None,
            notes: HashMap::new(),
        };
        show.add_scene(mk("verse"));
        show.add_scene(mk("chorus"));
        show.add_scene(mk("verse")); // upsert, not a duplicate
        assert_eq!(show.len(), 2);
        assert_eq!(show.names(), vec!["verse", "chorus"]);
        assert!(show.get("chorus").is_some());
        assert!(show.remove("verse"));
        assert!(!show.remove("verse"));
        assert_eq!(show.len(), 1);
    }
}
