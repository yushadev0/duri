//! ffmpeg itself: downloaded on first use, after the person agrees, into
//! %LOCALAPPDATA%\Duri\ffmpeg (not the roaming profile: it is ~170 MB).
//!
//! The package is a trimmed copy of BtbN's LGPL build (ffmpeg, ffprobe and
//! their DLLs, plus its license and a README pointing at the sources),
//! attached to a release of Duri's own repository. BtbN replaces its builds
//! every day, so a pinned URL there would break. The SHA-256 below is
//! checked before anything is unpacked.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const VERSION: &str = "9.0";
pub const PACKAGE_URL: &str =
    "https://github.com/yushadev0/duri/releases/download/ffmpeg-9.0/ffmpeg-9.0-win64-lgpl.zip";
pub const PACKAGE_SHA256: &str = "438227fabbc3cd94828db4700e93b441dca1b85543be97e57da268ccdb52dd19";
/// Shown before downloading ("66 MB").
pub const PACKAGE_BYTES: u64 = 66_513_201;

/// Marks a complete install; written last.
const MARKER: &str = "duri-ffmpeg-version.txt";

pub fn ffmpeg(dir: &Path) -> PathBuf {
    dir.join("ffmpeg.exe")
}

pub fn ffprobe(dir: &Path) -> PathBuf {
    dir.join("ffprobe.exe")
}

pub fn installed(dir: &Path) -> bool {
    ffmpeg(dir).is_file()
        && ffprobe(dir).is_file()
        && std::fs::read_to_string(dir.join(MARKER)).is_ok_and(|v| v.trim() == VERSION)
}

pub fn sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Unpacks the package's files (flat, no folders) into `dir`.
fn unpack(zip_path: &Path, dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Only a plain file name: never let an entry write outside `dir`.
        let Some(name) = entry.enclosed_name().and_then(|p| p.file_name().map(PathBuf::from)) else {
            continue;
        };
        let mut out = std::fs::File::create(dir.join(name)).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Downloads, checks and unpacks ffmpeg into `dir`. `progress` gets 0…1 and
/// returns false to cancel. Leaves nothing half-installed behind.
pub fn install(dir: &Path, mut progress: impl FnMut(f64) -> bool) -> Result<(), String> {
    let parent = dir.parent().ok_or("no folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let zip_path = parent.join("ffmpeg-download.zip");
    let staging = parent.join("ffmpeg-new");
    let cleanup = || {
        let _ = std::fs::remove_file(&zip_path);
        let _ = std::fs::remove_dir_all(&staging);
    };
    cleanup();

    let result = (|| {
        crate::http::download(PACKAGE_URL, &zip_path, |done, total| {
            let total = total.unwrap_or(PACKAGE_BYTES).max(1);
            progress((done as f64 / total as f64).min(1.0))
        })?;
        let hash = sha256_hex(&zip_path)?;
        if !hash.eq_ignore_ascii_case(PACKAGE_SHA256) {
            return Err("the download is damaged (checksum mismatch)".to_string());
        }
        unpack(&zip_path, &staging)?;
        if !ffmpeg(&staging).is_file() || !ffprobe(&staging).is_file() {
            return Err("the package has no ffmpeg.exe".to_string());
        }
        std::fs::write(staging.join(MARKER), VERSION).map_err(|e| e.to_string())?;
        remove(dir)?;
        std::fs::rename(&staging, dir).map_err(|e| e.to_string())
    })();
    cleanup();
    result
}

pub fn remove(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

#[cfg(windows)]
pub fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    /// CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS: no console flash, and
    /// a long encode doesn't make the PC sluggish.
    const FLAGS: u32 = 0x0800_0000 | 0x0000_4000;
    cmd.creation_flags(FLAGS)
}

#[cfg(not(windows))]
pub fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Runs a short command and reports whether it exited cleanly in time.
fn succeeds(program: &Path, args: &[&str], timeout: Duration) -> bool {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let Ok(mut child) = hidden(&mut cmd).spawn() else { return false };
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The first H.264 encoder that really works here. Hardware encoders are
/// listed even without the hardware (or with a too-old driver), so each is
/// tried on half a second of black frames.
pub fn detect_h264(dir: &Path) -> &'static str {
    let ffmpeg = ffmpeg(dir);
    for encoder in super::formats::H264_ENCODERS {
        let args = [
            "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=black:s=320x240:r=30:d=0.5",
            "-c:v", encoder, "-b:v", "1M", "-f", "null", "-",
        ];
        if succeeds(&ffmpeg, &args, Duration::from_secs(15)) {
            return encoder;
        }
    }
    "libopenh264"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unpacks_flat_and_ignores_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("p.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("ffmpeg.exe", opts).unwrap();
            w.write_all(b"x").unwrap();
            w.start_file("../evil.txt", opts).unwrap();
            w.write_all(b"y").unwrap();
            w.finish().unwrap();
        }
        let out = tmp.path().join("out");
        unpack(&zip_path, &out).unwrap();
        assert!(out.join("ffmpeg.exe").is_file());
        assert!(!tmp.path().join("evil.txt").exists());
    }

    #[test]
    fn hashes_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_hex(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn install_marker_decides() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::write(d.join("ffmpeg.exe"), b"").unwrap();
        std::fs::write(d.join("ffprobe.exe"), b"").unwrap();
        assert!(!installed(d));
        std::fs::write(d.join(MARKER), VERSION).unwrap();
        assert!(installed(d));
    }
}
