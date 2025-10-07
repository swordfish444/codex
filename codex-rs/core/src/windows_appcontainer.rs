#![cfg(windows)]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::ptr::null_mut;

use tokio::process::Child;
use tokio::process::Command;
use tracing::trace;

use crate::protocol::SandboxPolicy;
use crate::spawn::CODEX_SANDBOX_ENV_VAR;
use crate::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use crate::spawn::StdioPolicy;

use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::Security::Authorization::DACL_SECURITY_INFORMATION;
use windows::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows::Win32::Security::Authorization::GetNamedSecurityInfoW;
use windows::Win32::Security::Authorization::OBJECT_INHERIT_ACE;
use windows::Win32::Security::Authorization::SE_FILE_OBJECT;
use windows::Win32::Security::Authorization::SET_ACCESS;
use windows::Win32::Security::Authorization::SUB_CONTAINERS_AND_OBJECTS_INHERIT;
use windows::Win32::Security::Authorization::SetEntriesInAclW;
use windows::Win32::Security::Authorization::SetNamedSecurityInfoW;
use windows::Win32::Security::Authorization::TRUSTEE_FORM;
use windows::Win32::Security::Authorization::TRUSTEE_IS_UNKNOWN;
use windows::Win32::Security::Authorization::TRUSTEE_W;
use windows::Win32::Security::ConvertStringSidToSidW;
use windows::Win32::Security::CreateAppContainerProfile;
use windows::Win32::Security::DeriveAppContainerSidFromAppContainerName;
use windows::Win32::Security::FreeSid;
use windows::Win32::Security::PSID;
use windows::Win32::Security::SECURITY_CAPABILITIES;
use windows::Win32::Security::SID_AND_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
use windows::Win32::System::Memory::GetProcessHeap;
use windows::Win32::System::Memory::HEAP_ZERO_MEMORY;
use windows::Win32::System::Memory::HeapAlloc;
use windows::Win32::System::Memory::HeapFree;
use windows::Win32::System::Memory::LocalFree;
use windows::Win32::System::Threading::DeleteProcThreadAttributeList;
use windows::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT;
use windows::Win32::System::Threading::InitializeProcThreadAttributeList;
use windows::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_LIST;
use windows::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES;
use windows::Win32::System::Threading::UpdateProcThreadAttribute;
use windows::core::PCWSTR;
use windows::core::PWSTR;

/// Friendly name for the profile we create on-demand.
const WINDOWS_APPCONTAINER_PROFILE_NAME: &str = "codex_appcontainer";
const WINDOWS_APPCONTAINER_PROFILE_DESC: &str = "Codex Windows AppContainer profile";
/// Marker injected into the child so downstream tools can detect the sandbox.
const WINDOWS_APPCONTAINER_SANDBOX_VALUE: &str = "windows_appcontainer";
/// Capability SID strings that unlock outbound networking when the policy allows it.
const INTERNET_CLIENT_SID: &str = "S-1-15-3-1";
const PRIVATE_NETWORK_CLIENT_SID: &str = "S-1-15-3-3";

/// Runs the provided command inside an AppContainer sandbox that mirrors the
/// sandbox policy Codex already uses on macOS seatbelt and Linux Landlock. The
/// Windows sandbox flow is intentionally verbose so future contributors can map
/// each Windows API call to the equivalent behavior in the other platforms.
pub async fn spawn_command_under_windows_appcontainer(
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    mut env: HashMap<String, String>,
) -> io::Result<Child> {
    trace!("windows appcontainer sandbox command = {:?}", command);

    let (program, rest) = command
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "command args are empty"))?;

    // Windows requires a named profile before it will create an AppContainer
    // token, so we create-or-open the profile and then derive the SID that we
    // will hand to CreateProcess via the extended startup info structure.
    ensure_appcontainer_profile()?;
    let mut sid = derive_appcontainer_sid()?;
    // Capabilities translate Codex' sandbox policy knobs (for now just
    // networking) into Windows capability SIDs that can be attached to the
    // token. When the policy does not require a capability the vector is empty
    // and UpdateProcThreadAttribute receives a null pointer instead.
    let mut capability_sids = build_capabilities(sandbox_policy)?;
    // The attribute list owns the SECURITY_CAPABILITIES struct plus the heap
    // buffer required by UpdateProcThreadAttribute. Keeping it in a guard object
    // mirrors the RAII helpers we already use on the Unix sandboxes.
    let mut attribute_list = AttributeList::new(&mut sid, &mut capability_sids)?;

    // The Linux and macOS implementations pre-authorize the workspace so the
    // tool call can write to the expected roots. We replicate that behavior by
    // updating the directory ACLs for the derived AppContainer SID.
    configure_writable_roots(sandbox_policy, sandbox_policy_cwd, sid.sid())?;
    configure_writable_roots_for_command_cwd(&command_cwd, sid.sid())?;

    if !sandbox_policy.has_full_network_access() {
        env.insert(
            CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR.to_string(),
            "1".to_string(),
        );
    }
    env.insert(
        CODEX_SANDBOX_ENV_VAR.to_string(),
        WINDOWS_APPCONTAINER_SANDBOX_VALUE.to_string(),
    );

    let mut cmd = Command::new(program);
    cmd.args(rest);
    cmd.current_dir(command_cwd);
    cmd.env_clear();
    cmd.envs(env);
    apply_stdio_policy(&mut cmd, stdio_policy);
    cmd.kill_on_drop(true);

    unsafe {
        let std_cmd = cmd.as_std_mut();
        std_cmd.creation_flags(EXTENDED_STARTUPINFO_PRESENT);
        std_cmd.raw_attribute_list(attribute_list.as_mut_ptr());
    }

    let child = cmd.spawn();
    drop(attribute_list);
    child
}

fn apply_stdio_policy(cmd: &mut Command, policy: StdioPolicy) {
    match policy {
        StdioPolicy::RedirectForShellTool => {
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
        }
        StdioPolicy::Inherit => {
            cmd.stdin(std::process::Stdio::inherit());
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::inherit());
        }
    }
}

/// Converts a UTF-8 string into a Windows-compatible UTF-16 buffer with a
/// trailing nul byte. The helper keeps the conversion close to the code that
/// owns the literal strings so maintenance is straightforward.
fn to_wide<S: AsRef<OsStr>>(s: S) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Creates the AppContainer profile if it does not already exist. Windows keeps
/// track of AppContainer profiles globally for the user account, so subsequent
/// calls simply observe `ERROR_ALREADY_EXISTS` and continue.
fn ensure_appcontainer_profile() -> io::Result<()> {
    unsafe {
        let name = to_wide(WINDOWS_APPCONTAINER_PROFILE_NAME);
        let desc = to_wide(WINDOWS_APPCONTAINER_PROFILE_DESC);
        let hr = CreateAppContainerProfile(
            PCWSTR(name.as_ptr()),
            PCWSTR(name.as_ptr()),
            PCWSTR(desc.as_ptr()),
            null_mut(),
            0,
            null_mut(),
        );
        if let Err(error) = hr {
            let already_exists = WIN32_ERROR::from(ERROR_ALREADY_EXISTS);
            if GetLastError() != already_exists {
                return Err(io::Error::from_raw_os_error(error.code().0));
            }
        }
    }
    Ok(())
}

/// Small RAII wrapper around the derived AppContainer SID so we always release
/// it via `FreeSid` when the sandbox scaffolding is dropped.
struct SidHandle {
    ptr: PSID,
}

impl SidHandle {
    fn sid(&self) -> PSID {
        self.ptr
    }
}

impl Drop for SidHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                FreeSid(self.ptr);
            }
        }
    }
}

fn derive_appcontainer_sid() -> io::Result<SidHandle> {
    unsafe {
        let mut sid_ptr = null_mut();
        let name = to_wide(WINDOWS_APPCONTAINER_PROFILE_NAME);
        DeriveAppContainerSidFromAppContainerName(PCWSTR(name.as_ptr()), &mut sid_ptr)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0))?;
        Ok(SidHandle { ptr: sid_ptr })
    }
}

/// Holds capability SIDs that are allocated with `LocalAlloc`. Keeping the
/// pointers alive inside a struct simplifies cleanup.
struct CapabilitySid {
    sid: PSID,
}

impl CapabilitySid {
    fn new_from_string(value: &str) -> io::Result<Self> {
        unsafe {
            let mut sid_ptr = null_mut();
            let wide = to_wide(value);
            if ConvertStringSidToSidW(PCWSTR(wide.as_ptr()), &mut sid_ptr) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { sid: sid_ptr })
        }
    }

    fn sid_and_attributes(&self) -> SID_AND_ATTRIBUTES {
        SID_AND_ATTRIBUTES {
            Sid: self.sid,
            Attributes: 0,
        }
    }
}

impl Drop for CapabilitySid {
    fn drop(&mut self) {
        unsafe {
            if !self.sid.is_null() {
                LocalFree(self.sid as isize);
            }
        }
    }
}

fn build_capabilities(policy: &SandboxPolicy) -> io::Result<Vec<CapabilitySid>> {
    if policy.has_full_network_access() {
        // Matching the other platforms, enabling network access translates to
        // enabling both the public-internet capability and the private-network
        // capability. Each SID is allocated with LocalAlloc so the RAII wrapper
        // releases it automatically when the sandbox scaffolding drops.
        Ok(vec![
            CapabilitySid::new_from_string(INTERNET_CLIENT_SID)?,
            CapabilitySid::new_from_string(PRIVATE_NETWORK_CLIENT_SID)?,
        ])
    } else {
        Ok(Vec::new())
    }
}

/// Manages the Windows attribute list that injects `SECURITY_CAPABILITIES`
/// (the AppContainer SID plus capability SIDs) into `CreateProcessW`.
struct AttributeList<'a> {
    heap: HANDLE,
    buffer: *mut std::ffi::c_void,
    list: *mut PROC_THREAD_ATTRIBUTE_LIST,
    sec_caps: SECURITY_CAPABILITIES,
    sid_and_attributes: Vec<SID_AND_ATTRIBUTES>,
    #[allow(dead_code)]
    sid: &'a mut SidHandle,
    #[allow(dead_code)]
    capabilities: &'a mut Vec<CapabilitySid>,
}

impl<'a> AttributeList<'a> {
    fn new(sid: &'a mut SidHandle, caps: &'a mut Vec<CapabilitySid>) -> io::Result<Self> {
        unsafe {
            let mut list_size = 0usize;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut list_size);
            let heap = GetProcessHeap();
            if heap.is_invalid() {
                return Err(io::Error::last_os_error());
            }
            let buffer = HeapAlloc(heap, HEAP_ZERO_MEMORY, list_size);
            if buffer.is_null() {
                return Err(io::Error::last_os_error());
            }
            let list = buffer as *mut PROC_THREAD_ATTRIBUTE_LIST;
            InitializeProcThreadAttributeList(list, 1, 0, &mut list_size)
                .map_err(|e| io::Error::from_raw_os_error(e.code().0))?;

            let mut sid_and_attributes: Vec<SID_AND_ATTRIBUTES> =
                caps.iter().map(CapabilitySid::sid_and_attributes).collect();

            let mut sec_caps = SECURITY_CAPABILITIES {
                AppContainerSid: sid.sid(),
                Capabilities: if sid_and_attributes.is_empty() {
                    null_mut()
                } else {
                    sid_and_attributes.as_mut_ptr()
                },
                CapabilityCount: sid_and_attributes.len() as u32,
                Reserved: null_mut(),
            };

            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
                &mut sec_caps as *mut _ as *mut _,
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                null_mut(),
                null_mut(),
            )
            .map_err(|e| io::Error::from_raw_os_error(e.code().0))?;

            Ok(Self {
                heap,
                buffer,
                list,
                sec_caps,
                sid_and_attributes,
                sid,
                capabilities: caps,
            })
        }
    }

    fn as_mut_ptr(&mut self) -> *mut PROC_THREAD_ATTRIBUTE_LIST {
        self.list
    }
}

impl Drop for AttributeList<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.list.is_null() {
                DeleteProcThreadAttributeList(self.list);
            }
            if !self.heap.is_invalid() && !self.buffer.is_null() {
                HeapFree(self.heap, 0, self.buffer);
            }
        }
    }
}

/// Applies directory ACLs for every writable root described by the sandbox
/// policy. Granting explicit rights to the AppContainer SID mirrors how the
/// macOS and Linux sandboxes pre-authorize the workspace while leaving the rest
/// of the filesystem read-only.
fn configure_writable_roots(
    policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    sid: PSID,
) -> io::Result<()> {
    match policy {
        SandboxPolicy::DangerFullAccess => Ok(()),
        SandboxPolicy::ReadOnly => grant_path_with_flags(sandbox_policy_cwd, sid, false),
        SandboxPolicy::WorkspaceWrite { .. } => {
            let roots = policy.get_writable_roots_with_cwd(sandbox_policy_cwd);
            for writable in roots {
                grant_path_with_flags(&writable.root, sid, true)?;
                for ro in writable.read_only_subpaths {
                    grant_path_with_flags(&ro, sid, false)?;
                }
            }
            Ok(())
        }
    }
}

fn configure_writable_roots_for_command_cwd(command_cwd: &Path, sid: PSID) -> io::Result<()> {
    grant_path_with_flags(command_cwd, sid, true)
}

/// Adds an inheritable ACE for the AppContainer SID so the sandbox can reach
/// specific roots. The helper augments the existing DACL rather than
/// overwriting it so it is safe to call repeatedly.
fn grant_path_with_flags(path: &Path, sid: PSID, write: bool) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let wide = to_wide(path.as_os_str());
    unsafe {
        let mut existing_dacl = null_mut();
        let mut security_descriptor = null_mut();
        // Pull the current DACL so we can append our ACE without clobbering any
        // existing inheritance or user-specific access entries.
        let status = GetNamedSecurityInfoW(
            PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            &mut existing_dacl,
            null_mut(),
            &mut security_descriptor,
        );
        if status != WIN32_ERROR::from(ERROR_SUCCESS) {
            if !security_descriptor.is_null() {
                LocalFree(security_descriptor as isize);
            }
            return Err(io::Error::from_raw_os_error(status.0 as i32));
        }

        let permissions = if write {
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE
        } else {
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
        };
        let mut explicit = EXPLICIT_ACCESS_W {
            grfAccessPermissions: permissions,
            grfAccessMode: SET_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT | OBJECT_INHERIT_ACE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_FORM::TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: PWSTR(sid as *mut _),
                ..Default::default()
            },
        };

        let mut new_dacl = null_mut();
        let add_result = SetEntriesInAclW(1, &mut explicit, existing_dacl, &mut new_dacl);
        if add_result != WIN32_ERROR::from(ERROR_SUCCESS) {
            if !new_dacl.is_null() {
                LocalFree(new_dacl as isize);
            }
            if !security_descriptor.is_null() {
                LocalFree(security_descriptor as isize);
            }
            return Err(io::Error::from_raw_os_error(add_result.0 as i32));
        }

        let set_result = SetNamedSecurityInfoW(
            PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(new_dacl),
            None,
        );
        if set_result != WIN32_ERROR::from(ERROR_SUCCESS) {
            if !new_dacl.is_null() {
                LocalFree(new_dacl as isize);
            }
            if !security_descriptor.is_null() {
                LocalFree(security_descriptor as isize);
            }
            return Err(io::Error::from_raw_os_error(set_result.0 as i32));
        }

        if !new_dacl.is_null() {
            LocalFree(new_dacl as isize);
        }
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor as isize);
        }
    }

    Ok(())
}
