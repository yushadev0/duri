//! Converter: files dropped on the island become other formats through
//! ffmpeg, run as a separate process (see `tools` for where it comes from).
//!
//! One job runs at a time, below normal priority; ffmpeg already uses every
//! core. Output goes next to the source with a fresh name, never over an
//! existing file. The UI mirrors `convert://changed` snapshots.

pub mod formats;
pub mod probe;
pub mod tools;

use formats::{Advanced, Media, Quality, Target};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Tools {
    Missing,
    Downloading { progress: f64 },
    Ready,
    Failed { error: String },
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub input: String,
    /// File name, for display.
    pub name: String,
    pub target: String,
    pub status: Status,
    /// 0…1, None while unknown (images, the first moments of a job).
    pub progress: Option<f64>,
    pub output: Option<String>,
    pub error: Option<String>,
    /// Unix ms, for the time left.
    pub started_at: Option<u64>,
    #[serde(skip)]
    quality: Quality,
    #[serde(skip)]
    advanced: Advanced,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub tools: Tools,
    pub jobs: Vec<Job>,
    /// Download size, for the consent prompt.
    pub package_bytes: u64,
}

struct Inner {
    tools: Tools,
    jobs: Vec<Job>,
    next_id: u64,
    h264: Option<&'static str>,
    cancel_download: bool,
}

pub struct Convert {
    inner: Mutex<Inner>,
    wake: Condvar,
    /// The ffmpeg process that is running, so Cancel can stop it.
    child: Mutex<Option<Child>>,
    dir: PathBuf,
}

/// Finished jobs kept for the list.
const KEEP_FINISHED: usize = 20;
const EMIT_EVERY: Duration = Duration::from_millis(250);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Convert {
    fn snapshot(&self) -> Snapshot {
        let s = self.inner.lock().unwrap();
        Snapshot { tools: s.tools.clone(), jobs: s.jobs.clone(), package_bytes: tools::PACKAGE_BYTES }
    }

    fn update(&self, f: impl FnOnce(&mut Inner)) {
        if let Ok(mut s) = self.inner.lock() {
            f(&mut s);
        }
    }

    fn job(&self, id: u64, f: impl FnOnce(&mut Job)) {
        self.update(|s| {
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                f(j);
            }
        });
    }
}

fn emit(app: &AppHandle) {
    let snap = app.state::<Arc<Convert>>().snapshot();
    let _ = app.emit("convert://changed", snap);
}

pub fn init(app: &AppHandle) {
    let dir = app
        .path()
        .local_data_dir()
        .map(|d| d.join("Duri").join("ffmpeg"))
        .unwrap_or_else(|_| std::env::temp_dir().join("Duri").join("ffmpeg"));
    let state = Arc::new(Convert {
        inner: Mutex::new(Inner {
            tools: if tools::installed(&dir) { Tools::Ready } else { Tools::Missing },
            jobs: Vec::new(),
            next_id: 1,
            h264: None,
            cancel_download: false,
        }),
        wake: Condvar::new(),
        child: Mutex::new(None),
        dir,
    });
    app.manage(state.clone());
    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-convert".into())
        .spawn(move || worker(app, state))
        .expect("failed to spawn converter thread");
}

/// Runs queued jobs one after another.
fn worker(app: AppHandle, state: Arc<Convert>) {
    loop {
        let id = {
            let mut s = state.inner.lock().unwrap();
            loop {
                let next = (s.tools == Tools::Ready)
                    .then(|| s.jobs.iter().find(|j| j.status == Status::Queued).map(|j| j.id))
                    .flatten();
                if let Some(id) = next {
                    break id;
                }
                s = state.wake.wait(s).unwrap();
            }
        };
        state.job(id, |j| {
            j.status = Status::Running;
            j.started_at = Some(now_ms());
        });
        emit(&app);
        let result = run(&app, &state, id);
        let cancelled = state
            .inner
            .lock()
            .map(|s| s.jobs.iter().any(|j| j.id == id && j.status == Status::Cancelled))
            .unwrap_or(false);
        state.job(id, |j| match (&result, cancelled) {
            (_, true) => {}
            (Ok(out), false) => {
                j.status = Status::Done;
                j.progress = Some(1.0);
                j.output = Some(out.to_string_lossy().into_owned());
            }
            (Err(e), false) => {
                j.status = Status::Failed;
                j.error = Some(e.clone());
            }
        });
        if result.is_err() || cancelled {
            // A half-written file is no use to anyone.
            if let Ok(s) = state.inner.lock()
                && let Some(out) = s.jobs.iter().find(|j| j.id == id).and_then(|j| j.output.clone())
            {
                let _ = std::fs::remove_file(out);
            }
            state.job(id, |j| j.output = None);
        }
        state.update(prune);
        emit(&app);
    }
}

/// Keeps the newest finished jobs only.
fn prune(s: &mut Inner) {
    let finished = |j: &Job| matches!(j.status, Status::Done | Status::Failed | Status::Cancelled);
    let mut extra = s.jobs.iter().filter(|j| finished(j)).count().saturating_sub(KEEP_FINISHED);
    s.jobs.retain(|j| {
        if extra > 0 && finished(j) {
            extra -= 1;
            false
        } else {
            true
        }
    });
}

/// "clip.mp4" → "clip.webm", or "clip (1).webm" when that exists (and
/// always when it would be the input itself).
pub fn output_path(input: &Path, ext: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while exists(&candidate) || candidate == input {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

fn probe_file(dir: &Path, path: &str) -> Result<Media, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let mut cmd = Command::new(tools::ffprobe(dir));
        cmd.args(args).arg(path).stdin(Stdio::null());
        let out = tools::hidden(&mut cmd).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let json = run(&["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])?;
    let frames = if probe::needs_frame_count(&json) {
        run(&["-v", "error", "-count_packets", "-select_streams", "v:0", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"])
            .ok()
            .and_then(|s| s.trim().trim_end_matches(',').parse().ok())
    } else {
        None
    };
    Ok(probe::parse(&json, frames))
}

/// One progress line from `-progress pipe:1`: microseconds written so far.
pub fn progress_us(line: &str) -> Option<u64> {
    line.strip_prefix("out_time_us=")?.trim().parse().ok()
}

/// ffmpeg's last error lines, shortened for the island.
fn error_text(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let text = lines.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" · ");
    if text.is_empty() { "ffmpeg stopped".to_string() } else { text.chars().take(300).collect() }
}

fn run(app: &AppHandle, state: &Convert, id: u64) -> Result<PathBuf, String> {
    let (input, target_id, quality, advanced) = {
        let s = state.inner.lock().unwrap();
        let j = s.jobs.iter().find(|j| j.id == id).ok_or("gone")?;
        (j.input.clone(), j.target.clone(), j.quality, j.advanced.clone())
    };
    let media = probe_file(&state.dir, &input)?;
    let target = formats::find_target(&media, &target_id).ok_or("this file can't become that format")?;

    let needs_h264 = target.output == formats::Kind::Video && target.id != "webm";
    let mut h264 = state.inner.lock().unwrap().h264;
    if needs_h264 && h264.is_none() {
        let found = tools::detect_h264(&state.dir);
        state.update(|s| s.h264 = Some(found));
        h264 = Some(found);
    }

    let output = output_path(Path::new(&input), target.id, |p| p.exists());
    let out_str = output.to_string_lossy().into_owned();
    state.job(id, |j| j.output = Some(out_str.clone()));
    let detected = h264.unwrap_or(formats::SOFTWARE_H264);
    let job = formats::Job { input: &input, output: &out_str, media: &media, target, quality, advanced: &advanced, h264: detected };
    let result = encode(app, state, id, &formats::build_args(&job), &media, &advanced, &output);
    // A GPU encoder can still fail on a particular file (size, profile, a
    // driver hiccup): try once more in software.
    let hardware = needs_h264 && formats::h264_for(&media, &advanced, detected) != formats::SOFTWARE_H264;
    if result.is_err() && hardware && !cancelled(state, id) {
        let _ = std::fs::remove_file(&output);
        state.job(id, |j| j.progress = None);
        let job = formats::Job { h264: formats::SOFTWARE_H264, ..job };
        return encode(app, state, id, &formats::build_args(&job), &media, &advanced, &output);
    }
    result
}

fn cancelled(state: &Convert, id: u64) -> bool {
    state.inner.lock().map(|s| s.jobs.iter().any(|j| j.id == id && j.status == Status::Cancelled)).unwrap_or(false)
}

/// Runs ffmpeg with `args`, reporting progress, until it writes `output`.
fn encode(
    app: &AppHandle,
    state: &Convert,
    id: u64,
    args: &[String],
    media: &Media,
    advanced: &Advanced,
    output: &Path,
) -> Result<PathBuf, String> {
    let duration_us = formats::output_duration(media, advanced).map(|d| d * 1_000_000.0);

    let mut cmd = Command::new(tools::ffmpeg(&state.dir));
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Holding the slot while checking means a Cancel either sees the child
    // or is seen here (it may have come while the encoder was being found).
    let mut slot = state.child.lock().unwrap();
    if cancelled(state, id) {
        return Err("cancelled".into());
    }
    let mut child = tools::hidden(&mut cmd).spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("no output")?;
    let mut stderr = child.stderr.take().ok_or("no output")?;
    *slot = Some(child);
    drop(slot);

    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let mut last_emit = Instant::now() - EMIT_EVERY;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let (Some(us), Some(total)) = (progress_us(&line), duration_us)
            && total > 0.0
        {
            let p = (us as f64 / total).clamp(0.0, 0.999);
            state.job(id, |j| j.progress = Some(p));
            if last_emit.elapsed() >= EMIT_EVERY {
                last_emit = Instant::now();
                emit(app);
            }
        }
    }
    let status = state.child.lock().unwrap().take().map(|mut c| c.wait());
    let stderr = errors.join().unwrap_or_default();
    match status {
        Some(Ok(s)) if s.success() && output.is_file() => Ok(output.to_path_buf()),
        Some(Ok(_)) | None => Err(error_text(&stderr)),
        Some(Err(e)) => Err(e.to_string()),
    }
}

// ---- commands ----

#[tauri::command]
pub fn convert_get(state: State<'_, Arc<Convert>>) -> Snapshot {
    state.snapshot()
}

/// Starts downloading ffmpeg (the person agreed to it in the island).
#[tauri::command]
pub fn convert_install(app: AppHandle, state: State<'_, Arc<Convert>>) {
    let state = state.inner().clone();
    {
        let mut s = state.inner.lock().unwrap();
        if matches!(s.tools, Tools::Downloading { .. } | Tools::Ready) {
            return;
        }
        s.tools = Tools::Downloading { progress: 0.0 };
        s.cancel_download = false;
    }
    emit(&app);
    std::thread::spawn(move || {
        let mut last_emit = Instant::now();
        let result = tools::install(&state.dir, |p| {
            let mut s = state.inner.lock().unwrap();
            if s.cancel_download {
                return false;
            }
            s.tools = Tools::Downloading { progress: p };
            drop(s);
            if last_emit.elapsed() >= EMIT_EVERY {
                last_emit = Instant::now();
                emit(&app);
            }
            true
        });
        state.update(|s| {
            s.tools = match result {
                Ok(()) => Tools::Ready,
                Err(_) if s.cancel_download => Tools::Missing,
                Err(error) => Tools::Failed { error },
            };
        });
        state.wake.notify_all();
        emit(&app);
    });
}

#[tauri::command]
pub fn convert_cancel_install(state: State<'_, Arc<Convert>>) {
    state.update(|s| s.cancel_download = true);
}

/// Removes ffmpeg again (Settings). Queued jobs wait for a new download.
#[tauri::command]
pub fn convert_uninstall(app: AppHandle, state: State<'_, Arc<Convert>>) -> Result<(), String> {
    if state.inner.lock().unwrap().jobs.iter().any(|j| j.status == Status::Running) {
        return Err("busy".into());
    }
    tools::remove(&state.dir)?;
    state.update(|s| {
        s.tools = Tools::Missing;
        s.h264 = None;
    });
    emit(&app);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probed {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub media: Media,
    pub targets: Vec<Target>,
    /// Why it can't be converted (a folder, not a media file…).
    pub error: Option<String>,
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// What each dropped file is and what it can become.
#[tauri::command]
pub async fn convert_probe(state: State<'_, Arc<Convert>>, paths: Vec<String>) -> Result<Vec<Probed>, String> {
    let state = state.inner().clone();
    if state.inner.lock().unwrap().tools != Tools::Ready {
        return Err("missing".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let meta = std::fs::metadata(&path);
                let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let name = file_name(&path);
                let (media, error) = match meta {
                    Ok(m) if m.is_dir() => (Media::default(), Some("folder".to_string())),
                    Err(e) => (Media::default(), Some(e.to_string())),
                    Ok(_) => match probe_file(&state.dir, &path) {
                        Ok(media) if media.kind.is_some() => (media, None),
                        Ok(media) => (media, Some("unsupported".to_string())),
                        Err(_) => (Media::default(), Some("unsupported".to_string())),
                    },
                };
                let targets = formats::targets_for(&media);
                Probed { path, name, bytes, media, targets, error }
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct StartItem {
    pub path: String,
    pub target: String,
}

#[tauri::command]
pub fn convert_start(
    app: AppHandle,
    state: State<'_, Arc<Convert>>,
    items: Vec<StartItem>,
    quality: Quality,
    advanced: Advanced,
) -> Vec<u64> {
    let mut ids = Vec::new();
    state.update(|s| {
        for item in items {
            let id = s.next_id;
            s.next_id += 1;
            ids.push(id);
            s.jobs.push(Job {
                id,
                name: file_name(&item.path),
                input: item.path,
                target: item.target,
                status: Status::Queued,
                progress: None,
                output: None,
                error: None,
                started_at: None,
                quality,
                advanced: advanced.clone(),
            });
        }
    });
    state.wake.notify_all();
    emit(&app);
    ids
}

#[tauri::command]
pub fn convert_cancel(app: AppHandle, state: State<'_, Arc<Convert>>, id: u64) {
    let mut running = false;
    state.job(id, |j| {
        running = j.status == Status::Running;
        if matches!(j.status, Status::Queued | Status::Running) {
            j.status = Status::Cancelled;
        }
    });
    if running && let Some(child) = state.child.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    emit(&app);
}

/// Forgets finished jobs.
#[tauri::command]
pub fn convert_clear(app: AppHandle, state: State<'_, Arc<Convert>>) {
    state.update(|s| s.jobs.retain(|j| matches!(j.status, Status::Queued | Status::Running)));
    emit(&app);
}

/// Shows a converted file in Explorer, or opens it.
#[tauri::command]
pub fn convert_open(app: AppHandle, path: String, reveal: bool) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if reveal {
        app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(&path, None::<&str>).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_never_overwrites() {
        let input = Path::new(r"C:\v\clip.mp4");
        assert_eq!(output_path(input, "webm", |_| false), PathBuf::from(r"C:\v\clip.webm"));
        // Same format: never the input itself.
        assert_eq!(output_path(input, "mp4", |_| false), PathBuf::from(r"C:\v\clip (1).mp4"));
        let taken = |p: &Path| p.ends_with("clip.webm") || p.ends_with("clip (1).webm");
        assert_eq!(output_path(input, "webm", taken), PathBuf::from(r"C:\v\clip (2).webm"));
    }

    #[test]
    fn reads_progress_lines() {
        assert_eq!(progress_us("out_time_us=1500000"), Some(1_500_000));
        assert_eq!(progress_us("out_time_us=N/A"), None);
        assert_eq!(progress_us("progress=continue"), None);
    }

    /// Runs every target and quality through the real ffmpeg Duri installed
    /// (%LOCALAPPDATA%\Duri\ffmpeg). Not part of the normal run:
    /// `cargo test --lib real_ffmpeg -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_ffmpeg_runs_every_recipe() {
        let dir = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("Duri").join("ffmpeg");
        assert!(tools::installed(&dir), "ffmpeg isn't installed");
        let tmp = tempfile::tempdir().unwrap();
        let make = |name: &str, args: &[&str]| {
            let out = tmp.path().join(name);
            let mut cmd = Command::new(tools::ffmpeg(&dir));
            cmd.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(&out);
            assert!(tools::hidden(&mut cmd).status().unwrap().success(), "making {name}");
            out
        };
        let lavfi = |src: &str| ["-f".to_string(), "lavfi".to_string(), "-i".to_string(), src.to_string()];
        let video = {
            let mut a = lavfi("testsrc2=s=640x360:r=30:d=2").to_vec();
            a.extend(lavfi("sine=f=440:d=2"));
            a.extend(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"].map(String::from));
            make("in-video.mkv", &a.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let inputs = [
            video,
            make("in-song.wav", &["-f", "lavfi", "-i", "sine=f=440:d=2"]),
            make("in-alpha.png", &["-f", "lavfi", "-i", "color=c=red@0.5:s=64x48,format=rgba", "-frames:v", "1"]),
            make("in-anim.gif", &["-f", "lavfi", "-i", "testsrc2=s=120x80:r=10:d=1"]),
        ];
        // DURI_H264=h264_mf (or libopenh264, h264_qsv…) tries another encoder.
        let forced = std::env::var("DURI_H264").ok();
        let detected = tools::detect_h264(&dir);
        let h264 = forced.as_deref().unwrap_or(detected);
        println!("H.264 encoder: {h264}");
        let mut failures = Vec::new();
        let mut runs = 0;
        for input in &inputs {
            let input_s = input.to_string_lossy().into_owned();
            let media = probe_file(&dir, &input_s).unwrap();
            assert!(media.kind.is_some(), "{input_s} not recognised");
            for target in formats::targets_for(&media) {
                for quality in [Quality::High, Quality::Balanced, Quality::Small] {
                    for advanced in [Advanced::default(), Advanced { start: Some(0.5), max_width: Some(32), max_height: Some(240), fps: Some(15.0), audio_bitrate: Some(96), mono: true, ..Default::default() }] {
                        let output = output_path(input, target.id, |p| p.exists());
                        let out_s = output.to_string_lossy().into_owned();
                        let job = formats::Job { input: &input_s, output: &out_s, media: &media, target, quality, advanced: &advanced, h264 };
                        let args = formats::build_args(&job);
                        let mut cmd = Command::new(tools::ffmpeg(&dir));
                        cmd.args(&args).stdout(Stdio::null());
                        let out = tools::hidden(&mut cmd).output().unwrap();
                        runs += 1;
                        let ok = out.status.success() && std::fs::metadata(&output).is_ok_and(|m| m.len() > 0);
                        if !ok {
                            failures.push(format!(
                                "{:?} → {} {:?}: {}",
                                input.file_name().unwrap(),
                                target.id,
                                quality,
                                error_text(&String::from_utf8_lossy(&out.stderr))
                            ));
                        }
                        let _ = std::fs::remove_file(&output);
                    }
                }
            }
        }
        println!("{runs} conversions, {} failed", failures.len());
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn error_text_keeps_the_last_lines() {
        assert_eq!(error_text("a\n\nb\nc\n"), "b · c");
        assert_eq!(error_text(""), "ffmpeg stopped");
    }
}
