//! Video Chat's engine: its Node API (downloads, transcription, AI clip plans, rendering), which the pane talks to on
//! http://127.0.0.1:4100. Canopy starts it when it isn't running already (the web page may have started it), for this
//! PC only, and stops it on the way out unless it's still busy.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// The engine's port: 4100, as the web page expects, unless `VIDEO_CHAT_PORT` says otherwise (tests).
fn port() -> u16 {
    std::env::var("VIDEO_CHAT_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4100)
}
/// Where Video Chat lives on the owner's PC, unless `VIDEO_CHAT_DIR` or the pane's folder setting says otherwise.
const DEFAULT_FOLDER: &str = r"D:\projects\AppsToIntegrate\video chat";

/// The engine Canopy started, if it did (one per process, shared by every space's pane).
static STARTED: Mutex<Option<Child>> = Mutex::new(None);

fn settings_file() -> PathBuf {
    std::env::var_os("CANOPY_PROFILE")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("Canopy")))
        .unwrap_or_else(std::env::temp_dir)
        .join("videochat.json")
}

/// Whether `dir` is a Video Chat checkout (its engine's source is there).
pub fn is_checkout(dir: &Path) -> bool {
    dir.join("backend").join("src").join("main.ts").is_file()
}

/// The Video Chat folder: `VIDEO_CHAT_DIR`, else the one picked in the pane, else the usual place.
pub fn folder() -> Option<PathBuf> {
    let saved = std::fs::read(settings_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.get("folder").and_then(Value::as_str).map(PathBuf::from));
    std::env::var_os("VIDEO_CHAT_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain(saved)
        .chain([PathBuf::from(DEFAULT_FOLDER)])
        .find(|dir| is_checkout(dir))
}

pub fn remember_folder(dir: &Path) -> Result<(), String> {
    if !is_checkout(dir) {
        return Err("That folder isn't Video Chat (there's no backend\\src\\main.ts in it).".into());
    }
    let file = settings_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, serde_json::json!({ "folder": dir }).to_string()).map_err(|e| e.to_string())
}

/// Where the engine keeps its projects (`<folder>\backend\data\jobs`, or `VIDEO_CHAT_DATA\jobs` for an engine run
/// from another folder, as tests do).
pub fn jobs_dir(folder: &Path) -> PathBuf {
    match std::env::var_os("VIDEO_CHAT_DATA") {
        Some(data) => PathBuf::from(data).join("jobs"),
        None => folder.join("backend").join("data").join("jobs"),
    }
}

/// A project's source video on disk.
pub fn source_path(folder: &Path, job: &str) -> Option<PathBuf> {
    std::fs::read_dir(jobs_dir(folder).join(job))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_stem().is_some_and(|s| s == "source") && p.is_file())
}

/// A rendered file from its download address (`/api/long-to-short/jobs/<job>/clips/<name>`).
pub fn rendered_path(folder: &Path, download_url: &str) -> Option<PathBuf> {
    let mut parts = download_url.trim_end_matches('/').rsplit('/');
    let name = parts.next()?;
    parts.next().filter(|p| *p == "clips")?;
    let job = parts.next()?;
    let path = jobs_dir(folder).join(job).join(name);
    path.is_file().then_some(path)
}

/// Whether the engine answers.
pub fn healthy() -> bool {
    request("GET", "/api/health", None, Some(Duration::from_secs(3))).is_ok()
}

/// Whether Canopy started the engine that's running (rather than the web page or someone else).
pub fn started_here() -> bool {
    STARTED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_mut()
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
}

/// Starts the engine from `folder` for this PC only, its output in `log`. It takes a few seconds to answer.
pub fn start(folder: &Path, log: &Path) -> Result<(), String> {
    let mut started = STARTED.lock().unwrap_or_else(|p| p.into_inner());
    if started.as_mut().is_some_and(|child| matches!(child.try_wait(), Ok(None))) {
        return Ok(());
    }
    let ts_node = folder.join("node_modules").join("ts-node").join("dist").join("bin.js");
    if !ts_node.is_file() {
        return Err(format!(
            "Video Chat's packages aren't installed. Run npm install in {} once.",
            folder.display()
        ));
    }
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let out = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut command = Command::new("node");
    command
        .arg(&ts_node)
        .args(["--files", "src/main.ts"])
        .current_dir(folder.join("backend"))
        // This PC only, and quick to start: types were checked when the code was written.
        .env("HOST", "127.0.0.1")
        .env("PORT", port().to_string())
        .env("TS_NODE_TRANSPILE_ONLY", "true")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("Node couldn't start ({e}). Is Node.js installed?"))?;
    *started = Some(child);
    Ok(())
}

/// Stops the engine if Canopy started it.
pub fn stop() {
    if let Some(mut child) = STARTED.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// On the way out: stops the engine Canopy started, unless it's still downloading, transcribing or waiting to start a
/// scheduled download, which it then finishes in the background (the next Canopy picks it up).
pub fn shutdown() {
    if !started_here() {
        return;
    }
    let busy = |path: &str, test: &dyn Fn(&Value) -> bool| {
        request("GET", path, None, Some(Duration::from_secs(3)))
            .ok()
            .and_then(|v| v.as_array().cloned())
            .is_some_and(|items| items.iter().any(test))
    };
    let downloading = busy("/api/twitch/downloads", &|d| {
        matches!(d.get("status").and_then(Value::as_str), Some("downloading" | "queued" | "scheduled"))
    });
    let transcribing = busy("/api/boss/projects", &|p| {
        matches!(
            p.pointer("/transcription/state").and_then(Value::as_str),
            Some("running" | "queued")
        )
    });
    if downloading || transcribing {
        // Let it go on alone: dropping the handle leaves the process running.
        STARTED.lock().unwrap_or_else(|p| p.into_inner()).take();
    } else {
        stop();
    }
}

/// One JSON request to the engine. `timeout` limits the whole answer (None waits as long as it takes: a clip plan
/// on a 12-hour VOD can take many minutes). Errors carry the engine's own message when it gave one.
pub fn request(method: &str, path: &str, body: Option<&Value>, timeout: Option<Duration>) -> Result<Value, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port()));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(1500))
        .map_err(|_| "Video Chat's engine isn't running.".to_string())?;
    stream.set_read_timeout(timeout).map_err(|e| e.to_string())?;
    let body = body.map(Value::to_string).unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\nAccept: application/json\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        port(),
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .map_err(|e| format!("The engine stopped answering ({e})."))?;
    let mut reply = Vec::new();
    stream
        .read_to_end(&mut reply)
        .map_err(|e| format!("The engine stopped answering ({e})."))?;
    let (status, payload) = parse_response(&reply)?;
    let json: Value = if payload.iter().all(u8::is_ascii_whitespace) {
        Value::Null
    } else {
        serde_json::from_slice(&payload).map_err(|_| format!("The engine answered something unexpected ({status})."))?
    };
    if status >= 400 {
        return Err(error_message(&json).unwrap_or_else(|| format!("The engine said no ({status}).")));
    }
    Ok(json)
}

/// A Nest error's message: a string, or a list of them.
fn error_message(json: &Value) -> Option<String> {
    match json.get("message")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => Some(items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
        _ => None,
    }
}

/// The status and body of an HTTP/1.1 response, chunked or not.
fn parse_response(reply: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("The engine's answer was cut short.")?;
    let head = String::from_utf8_lossy(&reply[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or("The engine's answer had no status.")?;
    let body = &reply[split + 4..];
    let chunked = head
        .lines()
        .any(|line| line.to_ascii_lowercase().replace(' ', "") == "transfer-encoding:chunked");
    if !chunked {
        return Ok((status, body.to_vec()));
    }
    let (mut out, mut rest) = (Vec::new(), body);
    loop {
        let end = rest.windows(2).position(|w| w == b"\r\n").ok_or("A chunk was cut short.")?;
        let size = usize::from_str_radix(String::from_utf8_lossy(&rest[..end]).split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "A chunk had no size.")?;
        rest = &rest[end + 2..];
        if size == 0 {
            return Ok((status, out));
        }
        out.extend_from_slice(rest.get(..size).ok_or("A chunk was cut short.")?);
        rest = rest.get(size + 2..).unwrap_or_default();
    }
}

/// `%`-encodes a path segment (download keys and job ids are plain, but a key may hold anything).
pub fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_chunked_answers() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\n{\"a\":1}";
        assert_eq!(parse_response(plain).unwrap(), (200, b"{\"a\":1}".to_vec()));
        let chunked = b"HTTP/1.1 400 Bad Request\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"me\r\nf\r\nssage\":\"Nope.\"}\r\n0\r\n\r\n";
        let (status, body) = parse_response(chunked).unwrap();
        assert_eq!(status, 400);
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(error_message(&json).as_deref(), Some("Nope."));
        assert_eq!(
            error_message(&serde_json::json!({"message": ["one", "two"]})).as_deref(),
            Some("one two")
        );
    }

    /// Talks to a real engine: run one on a scratch folder (its projects land in `<folder>\data`), set
    /// `VIDEO_CHAT_PORT` to its port and `VIDEO_CHAT_TEST_VIDEO` to a short video, then `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn talks_to_a_running_engine() {
        let video = std::env::var("VIDEO_CHAT_TEST_VIDEO").expect("VIDEO_CHAT_TEST_VIDEO");
        assert!(healthy());
        let made = request("POST", "/api/boss/import-file", Some(&serde_json::json!({"path": video})), Some(Duration::from_secs(60))).unwrap();
        let job = made["jobId"].as_str().unwrap().to_owned();
        let projects = request("GET", "/api/boss/projects", None, Some(Duration::from_secs(10))).unwrap();
        let listed: Vec<crate::videochat::model::Summary> = serde_json::from_value(projects).unwrap();
        assert!(listed.iter().any(|p| p.job_id == job && p.duration_seconds.unwrap_or(0.0) > 1.0));
        let state = serde_json::json!({"messages": [{"role": "user", "content": "hi"}], "plan": null, "rendered": null, "settings": {}});
        request("PUT", &format!("/api/boss/jobs/{job}/state"), Some(&state), Some(Duration::from_secs(10))).unwrap();
        let back = request("GET", &format!("/api/boss/jobs/{job}/state"), None, Some(Duration::from_secs(10))).unwrap();
        assert_eq!(back["state"]["messages"][0]["content"], "hi");
        let error = request("POST", "/api/boss/import-file", Some(&serde_json::json!({"path": "nowhere.mp4"})), Some(Duration::from_secs(10))).unwrap_err();
        assert_eq!(error, "That file isn't there.");
        request("DELETE", &format!("/api/boss/jobs/{job}"), None, Some(Duration::from_secs(10))).unwrap();
    }

    #[test]
    fn finds_rendered_files_from_their_address() {
        let folder = std::env::temp_dir().join(format!("videochat-engine-{}", std::process::id()));
        let job = jobs_dir(&folder).join("abc");
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(job.join("short-1.mp4"), b"x").unwrap();
        std::fs::write(job.join("source.mkv"), b"x").unwrap();
        assert_eq!(
            rendered_path(&folder, "/api/long-to-short/jobs/abc/clips/short-1.mp4"),
            Some(job.join("short-1.mp4"))
        );
        assert_eq!(rendered_path(&folder, "/api/long-to-short/jobs/abc/clips/missing.mp4"), None);
        assert_eq!(source_path(&folder, "abc"), Some(job.join("source.mkv")));
        assert_eq!(encode("a b/ç"), "a%20b%2F%C3%A7");
        std::fs::remove_dir_all(folder).unwrap();
    }
}
