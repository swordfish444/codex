#![cfg(target_os = "windows")]

use crate::WINDOWS_SANDBOX_ARG1;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use codex_core::protocol::SandboxPolicy;
use codex_core::protocol::WritableRoot;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Security::Authorization::ACCESS_MODE;
use windows::Win32::Security::Authorization::ACL;
use windows::Win32::Security::Authorization::CONTAINER_INHERIT_ACE;
use windows::Win32::Security::Authorization::DACL_SECURITY_INFORMATION;
use windows::Win32::Security::Authorization::DENY_ACCESS;
use windows::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows::Win32::Security::Authorization::NO_MULTIPLE_TRUSTEE;
use windows::Win32::Security::Authorization::OBJECT_INHERIT_ACE;
use windows::Win32::Security::Authorization::SE_FILE_OBJECT;
use windows::Win32::Security::Authorization::SE_GROUP_ENABLED;
use windows::Win32::Security::Authorization::SET_ACCESS;
use windows::Win32::Security::Authorization::SetEntriesInAclW;
use windows::Win32::Security::Authorization::SetNamedSecurityInfoW;
use windows::Win32::Security::Authorization::TRUSTEE_IS_SID;
use windows::Win32::Security::Authorization::TRUSTEE_IS_UNKNOWN;
use windows::Win32::Security::Authorization::TRUSTEE_W;
use windows::Win32::Security::ConvertStringSidToSidW;
use windows::Win32::Security::CreateAppContainerProfile;
use windows::Win32::Security::DeleteAppContainerProfile;
use windows::Win32::Security::DeriveAppContainerSidFromAppContainerName;
use windows::Win32::Security::FreeSid;
use windows::Win32::Security::SECURITY_CAPABILITIES;
use windows::Win32::Security::SID;
use windows::Win32::Security::SID_AND_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::DELETE;
use windows::Win32::Storage::FileSystem::FILE_ADD_FILE;
use windows::Win32::Storage::FileSystem::FILE_ADD_SUBDIRECTORY;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
use windows::Win32::Storage::FileSystem::FILE_WRITE_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::FILE_WRITE_DATA;
use windows::Win32::Storage::FileSystem::FILE_WRITE_EA;
use windows::Win32::System::JobObjects::AssignProcessToJobObject;
use windows::Win32::System::JobObjects::CreateJobObjectW;
use windows::Win32::System::JobObjects::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
use windows::Win32::System::JobObjects::JOBOBJECT_BASIC_LIMIT_INFORMATION;
use windows::Win32::System::JobObjects::JOBOBJECT_EXTENDED_LIMIT_INFORMATION;
use windows::Win32::System::JobObjects::JobObjectExtendedLimitInformation;
use windows::Win32::System::JobObjects::SetInformationJobObject;
use windows::Win32::System::Memory::GetProcessHeap;
use windows::Win32::System::Memory::HEAP_ZERO_MEMORY;
use windows::Win32::System::Memory::HeapAlloc;
use windows::Win32::System::Memory::HeapFree;
use windows::Win32::System::Threading::CreateProcessW;
use windows::Win32::System::Threading::DeleteProcThreadAttributeList;
use windows::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT;
use windows::Win32::System::Threading::GetExitCodeProcess;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::Threading::InitializeProcThreadAttributeList;
use windows::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_LIST;
use windows::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES;
use windows::Win32::System::Threading::PROCESS_INFORMATION;
use windows::Win32::System::Threading::STARTUPINFOEXW;
use windows::Win32::System::Threading::UpdateProcThreadAttribute;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::WindowsProgramming::LocalFree;
use windows::core::PCWSTR;

#[derive(Parser, Debug)]
#[command(arg_required_else_help = true)]
struct WindowsSandboxCommand {
    /// Working directory to use when interpreting sandbox policy paths.
    sandbox_policy_cwd: PathBuf,

    /// Serialized sandbox policy as JSON.
    sandbox_policy: String,

    /// Command to execute under the sandbox.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

pub fn run_main() -> ! {
    match run_impl() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("codex-windows-sandbox failed: {err:#}");
            std::process::exit(1);
        }
    }
}

fn run_impl() -> Result<i32> {
    let args = collect_effective_args();
    let command = WindowsSandboxCommand::parse_from(args);

    let sandbox_policy: SandboxPolicy = serde_json::from_str(&command.sandbox_policy)
        .context("failed to parse sandbox policy JSON")?;

    run_sandboxed_process(
        &command.command,
        &command.sandbox_policy_cwd,
        sandbox_policy,
    )
}

fn collect_effective_args() -> impl Iterator<Item = String> {
    let mut iter = std::env::args();
    let _exe = iter.next();
    let mut remaining: Vec<String> = Vec::new();
    if let Some(first) = iter.next() {
        if first != WINDOWS_SANDBOX_ARG1 {
            remaining.push(first);
        }
    }
    remaining.extend(iter);
    std::iter::once(String::from("codex-windows-sandbox")).chain(remaining.into_iter())
}

fn run_sandboxed_process(
    command: &[String],
    policy_cwd: &Path,
    sandbox_policy: SandboxPolicy,
) -> Result<i32> {
    if command.is_empty() {
        return Err(anyhow!("no command specified"));
    }

    let profile = AppContainerProfile::create()?;
    let mut capabilities =
        SecurityCapabilitiesState::new(profile.sid(), sandbox_policy.has_full_network_access())?;
    let mut attribute_list = AttributeList::new()?;
    unsafe {
        attribute_list
            .update_security_capabilities(
                &mut capabilities.capabilities as *mut SECURITY_CAPABILITIES,
            )
            .context("UpdateProcThreadAttribute failed")?;
    }

    let writable_roots = sandbox_policy.get_writable_roots_with_cwd(policy_cwd);
    let allow_guards = apply_directory_permissions(&writable_roots, profile.sid())?;
    let deny_guards = apply_read_only_overrides(&writable_roots, profile.sid())?;

    let mut startup_info: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup_info.lpAttributeList = attribute_list.as_ptr();

    let mut command_line = build_command_line(command)?;
    let mut process_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    unsafe {
        CreateProcessW(
            PCWSTR(std::ptr::null()),
            command_line.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            true,
            EXTENDED_STARTUPINFO_PRESENT,
            std::ptr::null_mut(),
            PCWSTR(std::ptr::null()),
            &mut startup_info.StartupInfo,
            &mut process_info,
        )
        .ok()
        .context("CreateProcessW failed")?;
    }

    let process_handles = ProcessHandles::new(process_info);
    let job = JobObject::new()?;
    unsafe {
        job.assign(process_handles.process)?;
    }

    let wait = unsafe { WaitForSingleObject(process_handles.process, INFINITE) };
    if wait != WAIT_OBJECT_0 {
        return Err(anyhow!("WaitForSingleObject failed: {wait}"));
    }

    let mut exit_code: u32 = 0;
    unsafe {
        GetExitCodeProcess(process_handles.process, &mut exit_code)
            .ok()
            .context("GetExitCodeProcess failed")?;
    }

    drop(job);
    drop(process_handles);
    drop(deny_guards);
    drop(allow_guards);
    drop(attribute_list);
    drop(capabilities);

    Ok(exit_code as i32)
}

fn apply_directory_permissions(
    roots: &[WritableRoot],
    sid: *mut SID,
) -> Result<Vec<DirectoryAclGuard>> {
    let mut guards = Vec::new();
    for root in roots {
        guards.push(apply_acl(
            &root.root,
            sid,
            SET_ACCESS,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
            OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
        )?);
    }
    Ok(guards)
}

fn apply_read_only_overrides(
    roots: &[WritableRoot],
    sid: *mut SID,
) -> Result<Vec<DirectoryAclGuard>> {
    let mut guards = Vec::new();
    for root in roots {
        for ro in &root.read_only_subpaths {
            guards.push(apply_acl(
                ro,
                sid,
                DENY_ACCESS,
                FILE_GENERIC_WRITE
                    | FILE_ADD_FILE
                    | FILE_ADD_SUBDIRECTORY
                    | FILE_WRITE_ATTRIBUTES
                    | FILE_WRITE_DATA
                    | FILE_WRITE_EA,
                OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
            )?);
        }
    }
    Ok(guards)
}

fn apply_acl(
    path: &Path,
    sid: *mut SID,
    access_mode: ACCESS_MODE,
    access_mask: u32,
    inheritance: u32,
) -> Result<DirectoryAclGuard> {
    let wide_path = to_wide_path(path);
    unsafe {
        let mut security_descriptor = std::ptr::null_mut();
        let mut dacl_ptr: *mut ACL = std::ptr::null_mut();
        GetNamedSecurityInfoW(
            PCWSTR(wide_path.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl_ptr,
            std::ptr::null_mut(),
            &mut security_descriptor,
        )
        .ok()
        .with_context(|| format!("GetNamedSecurityInfoW failed for {}", path.display()))?;

        let original_acl = if !dacl_ptr.is_null() {
            let size = (*dacl_ptr).AclSize as usize;
            let mut buf = vec![0u8; size];
            std::ptr::copy_nonoverlapping(dacl_ptr as *const u8, buf.as_mut_ptr(), size);
            Some(buf)
        } else {
            None
        };

        let mut explicit = EXPLICIT_ACCESS_W::default();
        explicit.grfAccessPermissions = access_mask;
        explicit.grfAccessMode = access_mode;
        explicit.grfInheritance = inheritance;
        explicit.Trustee = TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.cast(),
        };

        let mut new_dacl = std::ptr::null_mut();
        SetEntriesInAclW(1, &mut explicit, dacl_ptr, &mut new_dacl)
            .ok()
            .with_context(|| format!("SetEntriesInAclW failed for {}", path.display()))?;

        SetNamedSecurityInfoW(
            PCWSTR(wide_path.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_dacl,
            std::ptr::null_mut(),
        )
        .ok()
        .with_context(|| format!("SetNamedSecurityInfoW failed for {}", path.display()))?;

        if !new_dacl.is_null() {
            LocalFree(new_dacl.cast());
        }
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor.cast());
        }

        Ok(DirectoryAclGuard::new(path.to_path_buf(), original_acl))
    }
}

fn build_command_line(command: &[String]) -> Result<Vec<u16>> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("command cannot be empty"))?;
    let mut cmd = Vec::new();
    append_program(&mut cmd, OsStr::new(program))?;
    for arg in args {
        cmd.push(' ' as u16);
        append_arg(&mut cmd, OsStr::new(arg))?;
    }
    cmd.push(0);
    Ok(cmd)
}

fn append_program(cmd: &mut Vec<u16>, program: &OsStr) -> Result<()> {
    let wide: Vec<u16> = program.encode_wide().collect();
    if wide.contains(&0) {
        return Err(anyhow!("program contains embedded nulls"));
    }
    cmd.push('"' as u16);
    cmd.extend(wide);
    cmd.push('"' as u16);
    Ok(())
}

fn append_arg(cmd: &mut Vec<u16>, arg: &OsStr) -> Result<()> {
    let wide: Vec<u16> = arg.encode_wide().collect();
    if wide.contains(&0) {
        return Err(anyhow!("argument contains embedded nulls"));
    }
    let needs_quotes =
        wide.is_empty() || wide.iter().any(|&c| c == b' ' as u16 || c == b'\t' as u16);
    if needs_quotes {
        cmd.push('"' as u16);
    }
    let mut backslashes = 0;
    for &ch in &wide {
        if ch == '\\' as u16 {
            backslashes += 1;
        } else {
            if ch == '"' as u16 {
                for _ in 0..=backslashes {
                    cmd.push('\\' as u16);
                }
            }
            backslashes = 0;
        }
        cmd.push(ch);
    }
    if needs_quotes {
        for _ in 0..backslashes {
            cmd.push('\\' as u16);
        }
        cmd.push('"' as u16);
    }
    Ok(())
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn to_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

struct AppContainerProfile {
    name_w: Vec<u16>,
    sid: *mut SID,
}

impl AppContainerProfile {
    fn create() -> Result<Self> {
        let name = format!("codex-appcontainer-{}", Uuid::new_v4());
        let desc = "Codex Windows sandbox";
        let name_w = to_wide(&name);
        let desc_w = to_wide(desc);

        unsafe {
            let hr = CreateAppContainerProfile(
                PCWSTR(name_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                PCWSTR(desc_w.as_ptr()),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            );
            if let Err(e) = hr {
                if e.code().0 != ERROR_ALREADY_EXISTS.0 as i32 {
                    return Err(e).context("CreateAppContainerProfile failed");
                }
            }
        }

        let mut sid = std::ptr::null_mut();
        unsafe {
            DeriveAppContainerSidFromAppContainerName(PCWSTR(name_w.as_ptr()), &mut sid)
                .ok()
                .context("DeriveAppContainerSidFromAppContainerName failed")?;
        }

        Ok(Self {
            name_w,
            sid: sid.cast(),
        })
    }

    fn sid(&self) -> *mut SID {
        self.sid
    }
}

impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = DeleteAppContainerProfile(PCWSTR(self.name_w.as_ptr())) {
                eprintln!("warning: failed to delete AppContainer profile: {err}");
            }
            FreeSid(self.sid.cast());
        }
    }
}

struct SecurityCapabilitiesState {
    capabilities: SECURITY_CAPABILITIES,
    capability_sids: Vec<*mut SID>,
    entries: Box<[SID_AND_ATTRIBUTES]>,
}

impl SecurityCapabilitiesState {
    fn new(appcontainer_sid: *mut SID, allow_network: bool) -> Result<Self> {
        let mut capability_sids = Vec::new();
        let mut entries_vec = Vec::new();

        if allow_network {
            let sid = create_capability_sid("S-1-15-3-1")?;
            entries_vec.push(SID_AND_ATTRIBUTES {
                Sid: sid.cast(),
                Attributes: SE_GROUP_ENABLED,
            });
            capability_sids.push(sid);
        }

        let mut entries = entries_vec.into_boxed_slice();
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: appcontainer_sid.cast(),
            Capabilities: if entries.is_empty() {
                std::ptr::null_mut()
            } else {
                entries.as_mut_ptr()
            },
            CapabilityCount: entries.len() as u32,
            Reserved: std::ptr::null_mut(),
        };

        Ok(Self {
            capabilities,
            capability_sids,
            entries,
        })
    }
}

impl Drop for SecurityCapabilitiesState {
    fn drop(&mut self) {
        for sid in &self.capability_sids {
            unsafe {
                LocalFree((*sid).cast());
            }
        }
    }
}

struct AttributeList {
    heap: HANDLE,
    buffer: *mut std::ffi::c_void,
    list: *mut PROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new() -> Result<Self> {
        unsafe {
            let mut size = 0;
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
            let heap = GetProcessHeap();
            let buffer = HeapAlloc(heap, HEAP_ZERO_MEMORY, size);
            if buffer.is_null() {
                return Err(anyhow!("HeapAlloc failed"));
            }
            let list = buffer as *mut PROC_THREAD_ATTRIBUTE_LIST;
            InitializeProcThreadAttributeList(list, 1, 0, &mut size)
                .ok()
                .context("InitializeProcThreadAttributeList failed")?;
            Ok(Self { heap, buffer, list })
        }
    }

    fn as_ptr(&self) -> *mut PROC_THREAD_ATTRIBUTE_LIST {
        self.list
    }

    unsafe fn update_security_capabilities(
        &mut self,
        caps: *mut SECURITY_CAPABILITIES,
    ) -> windows::core::Result<()> {
        UpdateProcThreadAttribute(
            self.list,
            0,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            caps.cast(),
            std::mem::size_of::<SECURITY_CAPABILITIES>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe {
            if !self.list.is_null() {
                DeleteProcThreadAttributeList(self.list);
            }
            if !self.buffer.is_null() {
                HeapFree(self.heap, 0, self.buffer);
            }
        }
    }
}

struct DirectoryAclGuard {
    path: PathBuf,
    original_acl: Option<Vec<u8>>,
}

impl DirectoryAclGuard {
    fn new(path: PathBuf, original_acl: Option<Vec<u8>>) -> Self {
        Self { path, original_acl }
    }
}

impl Drop for DirectoryAclGuard {
    fn drop(&mut self) {
        let wide_path = to_wide_path(&self.path);
        unsafe {
            let result = if let Some(acl) = self.original_acl.as_mut() {
                SetNamedSecurityInfoW(
                    PCWSTR(wide_path.as_ptr()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    acl.as_mut_ptr().cast(),
                    std::ptr::null_mut(),
                )
            } else {
                SetNamedSecurityInfoW(
                    PCWSTR(wide_path.as_ptr()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if let Err(err) = result {
                eprintln!(
                    "warning: failed to restore ACL for {}: {err}",
                    self.path.display()
                );
            }
        }
    }
}

struct ProcessHandles {
    process: HANDLE,
    thread: HANDLE,
}

impl ProcessHandles {
    fn new(info: PROCESS_INFORMATION) -> Self {
        Self {
            process: info.hProcess,
            thread: info.hThread,
        }
    }
}

impl Drop for ProcessHandles {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.thread);
            CloseHandle(self.process);
        }
    }
}

struct JobObject {
    handle: HANDLE,
}

impl JobObject {
    fn new() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(None, None)?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation = JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            };
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()
            .context("SetInformationJobObject failed")?;
            Ok(Self { handle })
        }
    }

    unsafe fn assign(&self, process: HANDLE) -> windows::core::Result<()> {
        AssignProcessToJobObject(self.handle, process)
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn create_capability_sid(sid: &str) -> Result<*mut SID> {
    let wide = to_wide(sid);
    let mut sid_ptr = std::ptr::null_mut();
    unsafe {
        ConvertStringSidToSidW(PCWSTR(wide.as_ptr()), &mut sid_ptr)
            .ok()
            .context("ConvertStringSidToSidW failed")?;
    }
    Ok(sid_ptr.cast())
}
