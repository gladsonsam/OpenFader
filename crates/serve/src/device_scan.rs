//! Keep native driver discovery bounded and outside the recorder host process.

use openfader_audio::{process_supervisor::ProcessSupervisor, DeviceInfo};
use std::{path::Path, time::Instant};

pub(super) fn in_helper(helper: &Path, deadline: Instant) -> Result<Vec<DeviceInfo>, String> {
    let mut child = ProcessSupervisor::spawn(helper, ["--list-devices"])
        .map_err(|e| format!("cannot start audio device scan: {e}"))?;
    let response = child
        .wait_for_ready(deadline, 256 * 1024, |_| true)
        .map_err(|e| format!("audio device scan failed: {e}"))?;
    let status = child.stop(deadline).map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("audio device scan helper exited unsuccessfully".into());
    }
    if response["state"] != "DEVICES" {
        return Err(response["error"]
            .as_str()
            .unwrap_or("invalid audio device scan response")
            .to_owned());
    }
    serde_json::from_value(response["devices"].clone())
        .map_err(|e| format!("invalid audio device list: {e}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    #[test]
    fn scan_decodes_layouts_and_terminates_a_stalled_driver() {
        let root =
            std::env::temp_dir().join(format!("openfader-device-scan-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nprintf '%s\\n' '{\"state\":\"DEVICES\",\"devices\":[{\"id\":\"fixture\",\"name\":\"fixture\",\"input_channels\":32,\"output_channels\":16,\"sample_rate\":48000}]}'\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let devices = in_helper(&helper, Instant::now() + Duration::from_secs(2)).unwrap();
        assert_eq!(
            (devices[0].input_channels, devices[0].output_channels),
            (32, 16)
        );
        fs::write(&helper, "#!/bin/sh\nexec sleep 30\n").unwrap();
        let start = Instant::now();
        assert!(in_helper(&helper, start + Duration::from_millis(100)).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        fs::remove_dir_all(root).unwrap();
    }
}
