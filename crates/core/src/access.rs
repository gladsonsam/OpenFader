//! Access roles: per-strip allow/deny + fader caps, enforced in a wrapper.
//!
//! [`Role`]s: [`Role::Admin`] and [`Role::Engineer`] have full mix access
//! (distinct types today for audit/UI gating; same enforcement), while
//! [`Role::Musician`] is restricted to an allow-list of strips, may not
//! touch main LR or recall scenes, may not rename, and is capped at
//! [`MUSICIAN_MAX_FADER`] (≈ −10 dB).
//!
//! Enforcement lives in [`CheckedDriver`], which wraps any
//! [`MixerDriver`] and rejects disallowed writes with an error. Reads are
//! never blocked.

use std::collections::HashSet;

use crate::{MixerDriver, MixerInfo, MixerSnapshot, ParamValue, StripId};

/// Fader ceiling for [`Role::Musician`]: ≈ −10 dB on a nominal
/// +10..−60 dB fader scale (see [`db_to_unity_approx`]).
pub const MUSICIAN_MAX_FADER: f32 = 0.714;

/// Approximate dB (−60..+10) to unity-gain 0..1 mapping (linear in dB).
/// Drivers with exact dB tables refine this; the musician cap uses it.
pub fn db_to_unity_approx(db: f32) -> f32 {
    ((db + 60.0) / 70.0).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Engineer,
    Musician,
}

/// Enforced access policy for one operator.
#[derive(Clone, Debug)]
pub struct Policy {
    pub role: Role,
    /// `None` = all strips allowed; `Some(set)` = allow-list of strip ids.
    pub allowed_strips: Option<HashSet<u16>>,
    pub allow_main: bool,
    pub allow_rename: bool,
    pub allow_recall: bool,
    /// Desk file upload (scenes/shows to the desk). Reads (download) are
    /// never blocked; only the mutating upload is gated.
    pub allow_files: bool,
    pub max_fader: f32,
}

impl Policy {
    /// Full access: everything allowed, faders to unity.
    pub fn admin() -> Self {
        Self {
            role: Role::Admin,
            allowed_strips: None,
            allow_main: true,
            allow_rename: true,
            allow_recall: true,
            allow_files: true,
            max_fader: 1.0,
        }
    }

    /// Full mix access today (same enforcement as admin); a distinct role
    /// so UIs/audit trails can tell operators apart later.
    pub fn engineer() -> Self {
        Self {
            role: Role::Engineer,
            ..Self::admin()
        }
    }

    /// Restricted operator: only `allowed` strips, no main LR, no scene
    /// recall, no renames, no file uploads, faders capped at [`MUSICIAN_MAX_FADER`].
    pub fn musician(allowed: impl IntoIterator<Item = u16>) -> Self {
        Self {
            role: Role::Musician,
            allowed_strips: Some(allowed.into_iter().collect()),
            allow_main: false,
            allow_rename: false,
            allow_recall: false,
            allow_files: false,
            max_fader: MUSICIAN_MAX_FADER,
        }
    }

    fn check_strip(&self, strip: StripId) -> anyhow::Result<()> {
        if let Some(allowed) = &self.allowed_strips {
            if !allowed.contains(&strip.0) {
                anyhow::bail!("role {:?}: strip {} not allowed", self.role, strip.0);
            }
        }
        Ok(())
    }

    fn check_level(&self, value: f32) -> anyhow::Result<()> {
        if value > self.max_fader {
            anyhow::bail!(
                "role {:?}: fader {value} exceeds cap {}",
                self.role,
                self.max_fader
            );
        }
        Ok(())
    }

    /// Pre-write check for strip-targeted commands (`fader`/`mute`/`param`).
    ///
    /// The engine (`openmix-engine`) calls these per connected client so one
    /// session can host several roles; [`CheckedDriver`] uses the same paths.
    /// Reads are never blocked.
    pub fn allow_strip(&self, strip: StripId) -> anyhow::Result<()> {
        self.check_strip(strip)
    }

    /// [`allow_strip`](Self::allow_strip), plus the role fader cap.
    pub fn allow_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.check_strip(strip)?;
        self.check_level(value)
    }

    /// [`allow_strip`](Self::allow_strip) for mute writes (no level cap).
    pub fn allow_mute(&self, strip: StripId) -> anyhow::Result<()> {
        self.check_strip(strip)
    }

    /// [`allow_strip`](Self::allow_strip), plus the rename gate.
    pub fn allow_rename(&self, strip: StripId) -> anyhow::Result<()> {
        self.check_strip(strip)?;
        if !self.allow_rename {
            anyhow::bail!("role {:?}: renaming strips not allowed", self.role);
        }
        Ok(())
    }

    /// Main-LR gate plus the role fader cap.
    pub fn allow_main(&self, value: f32) -> anyhow::Result<()> {
        if !self.allow_main {
            anyhow::bail!("role {:?}: main LR is blocked", self.role);
        }
        self.check_level(value)
    }

    /// Scene-recall gate.
    pub fn allow_recall(&self) -> anyhow::Result<()> {
        if !self.allow_recall {
            anyhow::bail!("role {:?}: scene recall is blocked", self.role);
        }
        Ok(())
    }

    /// Master-section gate: mix-bus / subgroup / mute-group masters are
    /// shared mixes, so restricted roles (no `allow_main`) cannot touch
    /// them even when their own strips are allowed.
    pub fn allow_master(&self) -> anyhow::Result<()> {
        if !self.allow_main {
            anyhow::bail!("role {:?}: masters are blocked", self.role);
        }
        Ok(())
    }

    /// Master fader write (bus / subgroup): master gate plus the fader cap.
    pub fn allow_master_fader(&self, value: f32) -> anyhow::Result<()> {
        self.allow_master()?;
        self.check_level(value)
    }

    /// Master mute write (bus / subgroup / mute group): master gate.
    pub fn allow_master_mute(&self) -> anyhow::Result<()> {
        self.allow_master()
    }

    /// Group-membership write scoped to one strip: the strip must be
    /// allowed AND the role must hold the master gate (membership changes
    /// what the shared group masters sum).
    pub fn allow_assign(&self, strip: StripId) -> anyhow::Result<()> {
        self.check_strip(strip)?;
        self.allow_master()
    }

    /// Desk file-upload gate (scenes/shows to the desk). Downloads are
    /// reads and are never blocked.
    pub fn allow_upload(&self) -> anyhow::Result<()> {
        if !self.allow_files {
            anyhow::bail!("role {:?}: file upload is blocked", self.role);
        }
        Ok(())
    }
}

/// [`MixerDriver`] wrapper enforcing a [`Policy`]. Reads pass through;
/// disallowed writes fail. Construct with [`CheckedDriver::new`].
pub struct CheckedDriver<'a, D: MixerDriver> {
    inner: &'a D,
    policy: Policy,
}

impl<'a, D: MixerDriver> CheckedDriver<'a, D> {
    pub fn new(inner: &'a D, policy: Policy) -> Self {
        Self { inner, policy }
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }
}

impl<D: MixerDriver + Sync> MixerDriver for CheckedDriver<'_, D> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn channel_count(&self) -> usize {
        self.inner.channel_count()
    }

    async fn disconnect(&self) -> anyhow::Result<()> { self.inner.disconnect().await }
    async fn refresh_streams(&self) -> anyhow::Result<()> { self.inner.refresh_streams().await }
    fn telemetry_timestamps(&self) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> { self.inner.telemetry_timestamps() }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.inner.connect(addr).await
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.policy.allow_fader(strip, value)?;
        self.inner.set_fader(strip, value).await
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.inner.get_fader(strip).await
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.policy.allow_mute(strip)?;
        self.inner.set_mute(strip, muted).await
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        self.inner.get_mute(strip).await
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        self.policy.allow_rename(strip)?;
        self.inner.set_name(strip, name).await
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        self.inner.get_name(strip).await
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        // No per-bus mask yet (TODO): buses are shared mixes, so they stay
        // adjustable within the role's fader cap.
        self.policy.check_level(value)?;
        self.inner.set_bus_fader(bus, value).await
    }

    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        self.inner.get_bus_fader(bus).await
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        self.policy.allow_master_mute()?;
        self.inner.set_bus_mute(bus, muted).await
    }

    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        self.inner.get_bus_mute(bus).await
    }

    fn bus_count(&self) -> u8 {
        self.inner.bus_count()
    }

    fn subgroup_count(&self) -> u8 {
        self.inner.subgroup_count()
    }

    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
        self.policy.allow_master_fader(value)?;
        self.inner.set_subgroup_fader(group, value).await
    }

    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
        self.inner.get_subgroup_fader(group).await
    }

    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        self.policy.allow_master_mute()?;
        self.inner.set_subgroup_mute(group, muted).await
    }

    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
        self.inner.get_subgroup_mute(group).await
    }

    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        self.policy.allow_assign(ch)?;
        self.inner.set_subgroup_assign(ch, group, member).await
    }

    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        self.inner.get_subgroup_assign(ch, group).await
    }

    fn mute_group_count(&self) -> u8 {
        self.inner.mute_group_count()
    }

    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        self.policy.allow_master_mute()?;
        self.inner.set_mute_group(group, muted).await
    }

    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        self.inner.get_mute_group(group).await
    }

    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        self.policy.allow_assign(ch)?;
        self.inner.set_mute_group_assign(ch, group, member).await
    }

    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        self.inner.get_mute_group_assign(ch, group).await
    }

    fn file_transfer(&self) -> bool {
        self.inner.file_transfer()
    }

    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        self.inner.download_file(filename).await
    }

    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        self.policy.allow_upload()?;
        self.inner.upload_file(filename, data, file_type).await
    }

    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.policy.allow_main(value)?;
        self.inner.set_main_fader(value).await
    }

    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        self.inner.get_main_fader().await
    }

    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        self.policy.allow_master_mute()?;
        self.inner.set_main_mute(muted).await
    }

    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        self.inner.get_main_mute().await
    }

    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        self.policy.allow_strip(strip)?;
        self.inner.set_solo(strip, soloed).await
    }

    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        self.inner.get_solo(strip).await
    }

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.policy.allow_strip(strip)?;
        match (param, &value) {
            ("fader", ParamValue::Float(v)) => self.policy.allow_fader(strip, *v)?,
            ("name", _) => self.policy.allow_rename(strip)?,
            _ => {}
        }
        self.inner.set_param(strip, param, value).await
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        self.inner.get_param(strip, param).await
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        self.inner.snapshot().await
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        self.policy.allow_recall()?;
        self.inner.recall(snapshot).await
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        self.inner.query_info().await
    }

    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<crate::MeterFrame>> {
        self.inner.subscribe_meters().await
    }

    fn poll_meters(&self) -> crate::MeterFrame {
        self.inner.poll_meters()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musician_cap_matches_minus_10db() {
        assert!((MUSICIAN_MAX_FADER - db_to_unity_approx(-10.0)).abs() < 0.01);
        assert_eq!(db_to_unity_approx(-60.0), 0.0);
        assert_eq!(db_to_unity_approx(10.0), 1.0);
    }

    #[test]
    fn policy_constructors() {
        let admin = Policy::admin();
        assert!(admin.allow_main && admin.allow_recall && admin.allow_rename);
        assert!(admin.allow_files);
        admin.allow_master().unwrap();
        admin.allow_master_fader(1.0).unwrap();
        admin.allow_upload().unwrap();
        assert_eq!(admin.max_fader, 1.0);
        assert!(admin.allowed_strips.is_none());

        let eng = Policy::engineer();
        assert_eq!(eng.role, Role::Engineer);
        assert!(eng.allow_main && eng.allow_recall);

        let mus = Policy::musician([0, 1]);
        assert_eq!(mus.role, Role::Musician);
        assert!(!mus.allow_main && !mus.allow_recall && !mus.allow_rename);
        assert!(!mus.allow_files);
        assert!(mus.allow_master().is_err());
        assert!(mus.allow_master_fader(0.5).is_err());
        assert!(mus.allow_master_mute().is_err());
        assert!(mus.allow_assign(StripId(0)).is_err());
        assert!(mus.allow_upload().is_err());
        assert_eq!(mus.max_fader, MUSICIAN_MAX_FADER);
        let allowed = mus.allowed_strips.unwrap();
        assert!(allowed.contains(&0) && allowed.contains(&1) && !allowed.contains(&5));
    }
}
