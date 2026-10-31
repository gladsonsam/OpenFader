//! Private, per-user macOS desktop bootstrap. The lock stays held until exit.
#[cfg(target_os = "macos")]
mod platform {
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        os::unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            io::AsRawFd,
        },
        path::PathBuf,
    };

    pub struct Owner {
        _lock: File,
        descriptor: PathBuf,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.descriptor);
        }
    }
    pub fn start(port: u16) -> anyhow::Result<Owner> {
        let root = PathBuf::from(
            std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME unavailable"))?,
        )
        .join("Library/Application Support/OpenFader/Recorder");
        fs::create_dir_all(&root)?;
        let meta = fs::symlink_metadata(&root)?;
        anyhow::ensure!(
            meta.is_dir()
                && !meta.file_type().is_symlink()
                && meta.uid() == unsafe { libc::geteuid() },
            "unsafe recorder directory"
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root.join("host.lock"))?;
        anyhow::ensure!(
            lock.metadata()?.uid() == unsafe { libc::geteuid() },
            "unsafe host lock"
        );
        anyhow::ensure!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "desktop host already running"
        );
        let mut bytes = [0u8; 48];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let hex = |b: &[u8]| b.iter().map(|v| format!("{v:02x}")).collect::<String>();
        let token = hex(&bytes[..32]);
        let instance = hex(&bytes[32..]);
        let descriptor = root.join("host.json");
        let tmp = root.join(format!(".host-{instance}.tmp"));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        serde_json::to_writer(
            &mut f,
            &serde_json::json!({"version":1,"instance_id":instance,"endpoint":format!("http://127.0.0.1:{port}"),"token":token}),
        )?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        fs::rename(&tmp, &descriptor)?;
        File::open(&root)?.sync_all()?;
        std::env::set_var("OPENFADER_RECORDER_TOKEN", token);
        std::env::set_var("OPENFADER_DESKTOP_INSTANCE", instance);
        Ok(Owner {
            _lock: lock,
            descriptor,
        })
    }
}
#[cfg(target_os = "macos")]
pub use platform::start;

#[cfg(target_os = "windows")]
mod windows {
    use std::{
        ffi::OsStr,
        fs::{self, File},
        io::Write,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle},
        },
        path::{Path, PathBuf},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Security::Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT,
        },
        Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
        Security::{
            GetAce, GetSecurityDescriptorControl, GetTokenInformation, TokenUser,
            ACCESS_ALLOWED_ACE, DACL_SECURITY_INFORMATION, INHERITED_ACE,
            OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_QUERY,
            TOKEN_USER,
        },
        Storage::FileSystem::{
            CreateDirectoryW, CreateFileW, MoveFileExW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, OPEN_ALWAYS,
            OPEN_EXISTING,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }
    fn win(ok: i32) -> anyhow::Result<()> {
        if ok == 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }
    struct Security(*mut core::ffi::c_void, String);
    impl Drop for Security {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    impl Security {
        fn current() -> anyhow::Result<Self> {
            unsafe {
                let mut token = ptr::null_mut();
                win(OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY,
                    &mut token,
                ))?;
                let mut len = 0;
                GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut len);
                // u64 storage gives TOKEN_USER its required alignment.
                let mut buf = vec![0u64; (len as usize).div_ceil(8)];
                let result = win(GetTokenInformation(
                    token,
                    TokenUser,
                    buf.as_mut_ptr().cast(),
                    len,
                    &mut len,
                ));
                CloseHandle(token);
                result?;
                let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
                let mut sid = ptr::null_mut();
                win(ConvertSidToStringSidW(user.User.Sid, &mut sid))?;
                let mut n = 0;
                while *sid.add(n) != 0 {
                    n += 1;
                }
                let sid_string = String::from_utf16_lossy(std::slice::from_raw_parts(sid, n));
                LocalFree(sid.cast());
                let sddl = wide(OsStr::new(&format!(
                    "O:{sid_string}D:P(A;OICI;FA;;;{sid_string})(A;OICI;FA;;;SY)"
                )));
                let mut sd = ptr::null_mut();
                win(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut sd,
                    ptr::null_mut(),
                ))?;
                Ok(Self(sd, sid_string))
            }
        }
        fn attrs(&self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.0,
                bInheritHandle: 0,
            }
        }
    }
    fn verify_security(file: &File, security: &Security) -> anyhow::Result<()> {
        unsafe {
            let mut owner = ptr::null_mut();
            let mut acl = ptr::null_mut();
            let mut sd = ptr::null_mut();
            let status = GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                &mut acl,
                ptr::null_mut(),
                &mut sd,
            );
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32).into());
            }
            let _guard = Security(sd, String::new());
            let sid_string = |sid| -> anyhow::Result<String> {
                let mut text = ptr::null_mut();
                win(ConvertSidToStringSidW(sid, &mut text))?;
                let mut len = 0;
                while *text.add(len) != 0 {
                    len += 1;
                }
                let value = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                LocalFree(text.cast());
                Ok(value)
            };
            anyhow::ensure!(
                !owner.is_null() && sid_string(owner)? == security.1,
                "unexpected recorder owner"
            );
            let mut control = 0;
            let mut revision = 0;
            win(GetSecurityDescriptorControl(
                sd,
                &mut control,
                &mut revision,
            ))?;
            anyhow::ensure!(
                control & SE_DACL_PROTECTED != 0 && !acl.is_null(),
                "unprotected recorder DACL"
            );
            anyhow::ensure!((*acl).AceCount == 2, "unexpected recorder ACE count");
            let mut user = false;
            let mut system = false;
            for i in 0..u32::from((*acl).AceCount) {
                let mut ace = ptr::null_mut();
                win(GetAce(acl, i, &mut ace))?;
                let ace = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
                anyhow::ensure!(
                    ace.Header.AceType == 0 && u32::from(ace.Header.AceFlags) & INHERITED_ACE == 0,
                    "unexpected recorder ACE"
                );
                let sid = sid_string((&ace.SidStart as *const u32).cast_mut().cast())?;
                if sid == security.1 {
                    user = true;
                } else if sid == "S-1-5-18" {
                    system = true;
                } else {
                    anyhow::bail!("unexpected recorder principal");
                }
            }
            anyhow::ensure!(user && system, "missing recorder principal");
            Ok(())
        }
    }
    fn pin_directory(path: &Path) -> anyhow::Result<File> {
        let h = unsafe {
            CreateFileW(
                wide(path.as_os_str()).as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let f = unsafe { File::from_raw_handle(h) };
        use std::os::windows::fs::MetadataExt;
        anyhow::ensure!(
            f.metadata()?.is_dir() && f.metadata()?.file_attributes() & 0x400 == 0,
            "unsafe recorder directory"
        );
        Ok(f)
    }
    fn reject_reparse(path: &Path) -> anyhow::Result<()> {
        use std::os::windows::fs::MetadataExt;
        if let Ok(m) = fs::symlink_metadata(path) {
            anyhow::ensure!(m.file_attributes() & 0x400 == 0, "unsafe recorder path");
        }
        Ok(())
    }
    fn file(path: &Path, security: &Security, disposition: u32) -> anyhow::Result<File> {
        reject_reparse(path)?;
        let h = unsafe {
            CreateFileW(
                wide(path.as_os_str()).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                &security.attrs(),
                disposition,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_handle(h) };
        use std::os::windows::fs::MetadataExt;
        anyhow::ensure!(
            file.metadata()?.file_attributes() & 0x400 == 0,
            "unsafe recorder file"
        );
        Ok(file)
    }
    pub struct Owner {
        _lock: File,
        _parent: File,
        _root: File,
        descriptor: PathBuf,
        instance: String,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            if reject_reparse(&self.descriptor).is_ok() {
                if let Ok(data) = fs::read(&self.descriptor) {
                    if serde_json::from_slice::<serde_json::Value>(&data)
                        .ok()
                        .and_then(|v| v.get("instance_id")?.as_str().map(str::to_owned))
                        .as_deref()
                        == Some(&self.instance)
                    {
                        let _ = fs::remove_file(&self.descriptor);
                    }
                }
            }
        }
    }
    pub fn start(port: u16) -> anyhow::Result<Owner> {
        let appdata = PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA unavailable"))?,
        );
        start_at(&appdata, port)
    }
    fn start_at(appdata: &Path, port: u16) -> anyhow::Result<Owner> {
        let security = Security::current()?;
        let parent = appdata.join("OpenFader");
        let root = parent.join("Recorder");
        reject_reparse(&parent)?;
        if unsafe { CreateDirectoryW(wide(parent.as_os_str()).as_ptr(), &security.attrs()) } == 0
            && !parent.is_dir()
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let parent_handle = pin_directory(&parent)?;
        reject_reparse(&root)?;
        if unsafe { CreateDirectoryW(wide(root.as_os_str()).as_ptr(), &security.attrs()) } == 0
            && !root.is_dir()
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let root_handle = pin_directory(&root)?;
        verify_security(&root_handle, &security)?;
        let lock = file(&root.join("host.lock"), &security, OPEN_ALWAYS)?;
        let mut bytes = [0u8; 48];
        anyhow::ensure!(
            unsafe {
                BCryptGenRandom(
                    ptr::null_mut(),
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            } >= 0,
            "system random generator failed"
        );
        let hex = |b: &[u8]| b.iter().map(|v| format!("{v:02x}")).collect::<String>();
        let token = hex(&bytes[..32]);
        let instance = hex(&bytes[32..]);
        let descriptor = root.join("host.json");
        let tmp = root.join(format!(".host-{instance}.tmp"));
        reject_reparse(&descriptor)?;
        let result = (|| -> anyhow::Result<()> {
            let mut f = file(&tmp, &security, CREATE_NEW)?;
            verify_security(&f, &security)?;
            serde_json::to_writer(
                &mut f,
                &serde_json::json!({"version":1,"instance_id":instance,"endpoint":format!("http://127.0.0.1:{port}"),"token":token}),
            )?;
            f.write_all(b"\n")?;
            f.sync_all()?;
            drop(f);
            win(unsafe {
                MoveFileExW(
                    wide(tmp.as_os_str()).as_ptr(),
                    wide(descriptor.as_os_str()).as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        std::env::set_var("OPENFADER_RECORDER_TOKEN", token);
        std::env::set_var("OPENFADER_DESKTOP_INSTANCE", &instance);
        Ok(Owner {
            _lock: lock,
            _parent: parent_handle,
            _root: root_handle,
            descriptor,
            instance,
        })
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn current_user_security_and_lock_lifecycle() {
            let security = Security::current().unwrap();
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                unsafe {
                    windows_sys::Win32::Security::GetSecurityDescriptorControl(
                        security.0,
                        &mut control,
                        &mut revision,
                    )
                },
                0
            );
            assert_ne!(control & windows_sys::Win32::Security::SE_DACL_PROTECTED, 0);
            let mut random = [0u8; 8];
            assert!(
                unsafe {
                    BCryptGenRandom(
                        ptr::null_mut(),
                        random.as_mut_ptr(),
                        8,
                        BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                    )
                } >= 0
            );
            let path = std::env::temp_dir()
                .join(format!("openfader-test-{:x}", u64::from_le_bytes(random)));
            fs::create_dir(&path).unwrap();
            let owner = start_at(&path, 1234).unwrap();
            let descriptor = path.join("OpenFader/Recorder/host.json");
            let descriptor_handle = File::open(&descriptor).unwrap();
            verify_security(&descriptor_handle, &security).unwrap();
            drop(descriptor_handle);
            let data: serde_json::Value =
                serde_json::from_slice(&fs::read(&descriptor).unwrap()).unwrap();
            assert_eq!(data["version"], 1);
            assert_eq!(data["endpoint"], "http://127.0.0.1:1234");
            assert_eq!(data["token"].as_str().unwrap().len(), 64);
            assert_eq!(data["instance_id"].as_str().unwrap().len(), 32);
            assert!(start_at(&path, 1234).is_err());
            assert!(pin_directory(&descriptor).is_err());
            drop(owner);
            assert!(!descriptor.exists());
            let link = path.join("reparse");
            match std::os::windows::fs::symlink_dir(&path, &link) {
                Ok(()) => {
                    assert!(pin_directory(&link).is_err());
                    fs::remove_dir(&link).unwrap();
                }
                Err(e) => eprintln!("Skipping reparse test: symlink creation unavailable: {e}"),
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
}
#[cfg(target_os = "windows")]
pub use windows::start;
