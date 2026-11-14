//! Hand a selection to a new Codex Desktop composer or a previously chosen Desktop chat.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use url::Url;

fn composer_url(task_dir: &Path, prompt: &str) -> Result<Url, String> {
    if !task_dir.is_absolute() || !task_dir.is_dir() {
        return Err(format!(
            "Codex needs an existing workspace folder: {}",
            task_dir.display()
        ));
    }
    let path = task_dir
        .to_str()
        .ok_or("Codex could not read the workspace path as text.")?;
    let mut url = Url::parse("codex://threads/new").expect("valid Codex composer URL");
    url.query_pairs_mut()
        .append_pair("path", path)
        .append_pair("prompt", prompt);
    Ok(url)
}

/// The Desktop app focuses a prefilled composer. It starts the turn when the user presses Send.
pub(super) fn open(task_dir: &Path, prompt: &str) -> Result<(), String> {
    open_link(composer_url(task_dir, prompt)?.as_str())
}

pub(super) fn valid_thread_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(position, byte)| {
            if matches!(position, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn background_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.stdin(Stdio::null());
    command
}

#[cfg(windows)]
fn desktop_cli() -> Option<PathBuf> {
    // A GUI app launched from Explorer does not inherit Codex Desktop's additions to PATH.
    let windows = PathBuf::from(std::env::var_os("SystemRoot")?);
    if !windows.is_absolute() {
        return None;
    }
    let powershell = windows.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    // This fixed query takes no user input. Only the native CLI receives chat IDs and prompts.
    let output = background_command(&powershell).args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
        "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); (Get-AppxPackage -Name OpenAI.Codex | Where-Object PublisherId -eq 2p2nqsd0c76g0 | Select-Object -First 1).InstallLocation"
    ]).output().ok()?;
    let location = PathBuf::from(std::str::from_utf8(&output.stdout).ok()?.trim());
    let cli = location.join("app/resources/codex.exe");
    (output.status.success() && location.is_absolute() && cli.is_file()).then_some(cli)
}

fn codex_cli() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(cli) = desktop_cli() {
        return Some(cli);
    }
    let path = std::env::var_os("PATH")?;
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|cli| cli.is_file())
}

fn queue_command(cli: &Path, thread_id: &str, prompt: &str) -> Command {
    let mut command = background_command(cli);
    command.args(["queue", "--thread", thread_id, "--message", prompt]);
    command
}

/// Called on the handoff worker. Queue just once, then ask the Desktop host to process its queue.
pub(super) fn queue(thread_id: &str, prompt: &str) -> Result<(), String> {
    if !valid_thread_id(thread_id) {
        return Err("Enter a Codex chat ID in UUID format.".into());
    }
    let cli = codex_cli().ok_or("Could not find the Codex CLI. Install Codex Desktop or put the native Codex executable on PATH.")?;
    let mut child = queue_command(&cli, thread_id, prompt)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start the Codex CLI: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = child.try_wait().map_err(|e| {
            format!(
                "Could not verify the queue result ({e}). Check the Codex chat before resending."
            )
        })?;
        if let Some(status) = status {
            if !status.success() {
                return Err(format!(
                    "Codex queue exited with {status}. Check the chat ID, Codex sign-in and chat before resending."
                ));
            }
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Codex did not acknowledge the handoff in time. Check the chat before resending."
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    open_link(&format!("codex://threads/wake/{thread_id}")).map_err(|e| {
        format!("The request is already queued. Open the selected chat in Codex to continue. {e}")
    })
}

#[cfg(windows)]
fn open_link(url: &str) -> Result<(), String> {
    use std::ptr::null;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: isize,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *const std::ffi::c_void, flags: u32) -> i32;
        fn CoUninitialize();
    }
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    // Shell protocol handlers can use COM. Balance both S_OK and S_FALSE on this worker thread.
    let initialized = unsafe { CoInitializeEx(null(), 2) } >= 0; // COINIT_APARTMENTTHREADED
    let result = unsafe { ShellExecuteW(0, null(), url.as_ptr(), null(), null(), 1) };
    if initialized {
        unsafe { CoUninitialize() };
    }
    match result {
        33.. => Ok(()),
        31 => Err("Windows has no handler for codex: links. Install or repair Codex Desktop and try again.".into()),
        _ => Err(format!("Windows could not open Codex Desktop (ShellExecute error {result}).")),
    }
}

#[cfg(not(windows))]
fn open_link(url: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(opener)
        .arg(url)
        .status()
        .map_err(|e| format!("Could not open Codex Desktop: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not open the codex: link. Check that Codex Desktop is installed ({status})."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_targets_one_uuid_and_passes_the_prompt_as_one_argument() {
        let id = "019550cd-a368-73d2-9e56-BaBb018FF199";
        assert!(valid_thread_id(id));
        assert!(!valid_thread_id(&format!("codex://threads/{id}")));
        assert!(!valid_thread_id(&id.replace('-', "")));
        assert!(!valid_thread_id(&id.replace('7', "z")));
        let prompt = "Read C:\\ملف & 2\\a.json\nKeep the user's \"voice\"! %PATH%";
        let command = queue_command(Path::new("codex.exe"), id, prompt);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["queue", "--thread", id, "--message", prompt]
        );
    }

    #[test]
    fn opens_a_new_composer_preserving_reserved_characters_and_unicode() {
        let dir = std::env::temp_dir();
        let prompt = "Read D:\\مرحبا & 100%\\a+b #1.json\nThen show the title?";
        let url = composer_url(&dir, prompt).unwrap();
        assert_eq!(
            (url.scheme(), url.host_str(), url.path()),
            ("codex", Some("threads"), "/new")
        );
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            (&*pairs[0].0, &*pairs[0].1),
            ("path", dir.to_str().unwrap())
        );
        assert_eq!((&*pairs[1].0, &*pairs[1].1), ("prompt", prompt));
        assert!(url.fragment().is_none());
    }
}
