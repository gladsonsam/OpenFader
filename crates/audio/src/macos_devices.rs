//! Read-only CoreAudio property enumeration. Does not instantiate audio units.
use crate::DeviceInfo;
use std::{
    ffi::{c_char, c_void, CStr},
    io, mem, ptr,
};
#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}
#[repr(C)]
struct Buffer {
    channels: u32,
    size: u32,
    data: *mut c_void,
}
#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        id: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(
        s: *const c_void,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> bool;
    fn CFRelease(s: *const c_void);
}
const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
fn property(id: u32, selector: u32, scope: u32) -> io::Result<Vec<u64>> {
    let address = Address {
        selector,
        scope,
        element: 0,
    };
    let mut size = 0;
    // SAFETY: address and size are live, correctly aligned values; no qualifier
    // is supplied and CoreAudio retains none of these pointers.
    let status = unsafe { AudioObjectGetPropertyDataSize(id, &address, 0, ptr::null(), &mut size) };
    if status != 0 || size > 1_048_576 {
        return Err(io::Error::other("CoreAudio property size unavailable"));
    }
    let mut data = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: data is aligned for all queried property types and has at least
    // the byte capacity returned above; size communicates that capacity.
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &address,
            0,
            ptr::null(),
            &mut size,
            data.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Err(io::Error::other("CoreAudio property unavailable"));
    }
    Ok(data)
}
fn string(id: u32, selector: u32) -> io::Result<String> {
    let data = property(id, selector, GLOBAL)?;
    let value = data.first().copied().unwrap_or(0) as *const c_void;
    if value.is_null() {
        return Err(io::Error::other("CoreAudio string missing"));
    }
    let mut buf = vec![0 as c_char; 4096];
    // SAFETY: these selectors return an owned CFStringRef; value is non-null
    // and buf is writable for the supplied capacity during the conversion.
    let ok = unsafe { CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as isize, 0x08000100) };
    // SAFETY: release the reference obtained from CoreAudio once, after use.
    unsafe { CFRelease(value) };
    if !ok {
        return Err(io::Error::other("CoreAudio string unavailable"));
    }
    // SAFETY: a successful CFStringGetCString writes a NUL-terminated string
    // within buf, which stays live while it is copied into an owned String.
    Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}
fn channels(id: u32, scope: u32) -> io::Result<u16> {
    let data = property(id, u32::from_be_bytes(*b"slay"), scope)?;
    if data.is_empty() {
        return Ok(0);
    }
    let count = data[0] as u32 as usize;
    let needed = 8usize
        .checked_add(
            count
                .checked_mul(mem::size_of::<Buffer>())
                .ok_or_else(|| io::Error::other("invalid buffers"))?,
        )
        .ok_or_else(|| io::Error::other("invalid buffers"))?;
    if needed > data.len() * 8 {
        return Err(io::Error::other("invalid CoreAudio buffer list"));
    }
    // SAFETY: on supported 64-bit macOS targets AudioBufferList places its
    // aligned AudioBuffer array at byte 8; needed validates the complete slice.
    // CoreAudio initializes the buffer list for the stream-configuration selector.
    let buffers = unsafe {
        std::slice::from_raw_parts(data.as_ptr().cast::<u8>().add(8).cast::<Buffer>(), count)
    };
    Ok(buffers.iter().fold(0u16, |n, b| {
        n.saturating_add(b.channels.min(u16::MAX as u32) as u16)
    }))
}
pub fn devices() -> io::Result<Vec<DeviceInfo>> {
    let data = property(1, u32::from_be_bytes(*b"dev#"), GLOBAL)?;
    let mut result = Vec::new();
    for word in data {
        for id in [word as u32, (word >> 32) as u32] {
            if id == 0 {
                continue;
            }
            let (Ok(input_channels), Ok(output_channels)) = (
                channels(id, u32::from_be_bytes(*b"inpt")),
                channels(id, u32::from_be_bytes(*b"outp")),
            ) else {
                continue;
            };
            let (Ok(name), Ok(uid), Ok(rate)) = (
                string(id, u32::from_be_bytes(*b"lnam")),
                string(id, u32::from_be_bytes(*b"uid ")),
                property(id, u32::from_be_bytes(*b"nsrt"), GLOBAL),
            ) else {
                continue;
            };
            let sample_rate = f64::from_bits(*rate.first().unwrap_or(&0));
            if !sample_rate.is_finite()
                || !(1.0..=u32::MAX as f64).contains(&sample_rate)
                || sample_rate.fract() != 0.0
            {
                continue;
            }
            result.push(DeviceInfo {
                id: format!("coreaudio:{uid}"),
                name,
                input_channels,
                output_channels,
                sample_rate: sample_rate as u32,
            });
        }
    }
    Ok(result)
}
