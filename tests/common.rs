use std::process::Command;

pub fn hyperfine_raw_command() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("joulex"));
    cmd.current_dir("tests/");
    cmd
}

pub fn hyperfine() -> assert_cmd::Command {
    assert_cmd::Command::from_std(hyperfine_raw_command())
}

#[allow(dead_code)]
pub fn is_rosetta() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let mut ret: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>();
        let name = std::ffi::CString::new("sysctl.proc_translated").unwrap();
        // SAFETY: sysctlbyname is a standard safe Darwin API.
        let res = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut ret as *mut _ as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if res == 0 && ret == 1 {
            return true;
        }
    }
    false
}
