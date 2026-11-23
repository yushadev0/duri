//! What each kind of file can turn into, and the ffmpeg arguments for it.
//! Pure functions, so every recipe is unit tested without ffmpeg.

use serde::{Deserialize, Serialize};

/// What a dropped file is, as far as converting goes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Audio,
    Image,
    /// An animated GIF or APNG: frames like a video, no sound.
    Animation,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    #[default]
    Balanced,
    Small,
}

impl Quality {
    /// Picks one of three values: (high, balanced, small).
    fn pick<T>(self, high: T, balanced: T, small: T) -> T {
        match self {
            Quality::High => high,
            Quality::Balanced => balanced,
            Quality::Small => small,
        }
    }
}

/// The Advanced panel. Every field is "keep the original" when unset.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Advanced {
    /// Video: largest output height (1080 = 1080p). Never upscales.
    pub max_height: Option<u32>,
    /// Images, GIFs: largest output width. Never upscales.
    pub max_width: Option<u32>,
    pub fps: Option<f64>,
    /// kbps
    pub audio_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub mono: bool,
    /// Video: drop the sound.
    pub mute: bool,
    /// Trim, in seconds from the start of the file.
    pub start: Option<f64>,
    pub end: Option<f64>,
}

impl Advanced {
    fn trims(&self) -> bool {
        self.start.is_some_and(|s| s > 0.0) || self.end.is_some()
    }
}

/// What ffprobe found in a file.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub kind: Option<Kind>,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    /// The picture has transparency (matters for JPG, which has none).
    pub alpha: bool,
}

/// A format someone can pick.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: &'static str,
    /// What comes out: an MP3 made from a video is audio.
    pub output: Kind,
    /// Quality presets do nothing (FLAC, PNG…): the UI hides them.
    pub lossless: bool,
}

const fn t(id: &'static str, output: Kind, lossless: bool) -> Target {
    Target { id, output, lossless }
}

const VIDEO_OUT: [Target; 5] = [
    t("mp4", Kind::Video, false),
    t("mkv", Kind::Video, false),
    t("webm", Kind::Video, false),
    t("mov", Kind::Video, false),
    t("gif", Kind::Animation, false),
];
const AUDIO_OUT: [Target; 7] = [
    t("mp3", Kind::Audio, false),
    t("m4a", Kind::Audio, false),
    t("wav", Kind::Audio, true),
    t("flac", Kind::Audio, true),
    t("ogg", Kind::Audio, false),
    t("opus", Kind::Audio, false),
    t("aiff", Kind::Audio, true),
];
const IMAGE_OUT: [Target; 8] = [
    t("png", Kind::Image, true),
    t("jpg", Kind::Image, false),
    t("webp", Kind::Image, false),
    t("avif", Kind::Image, false),
    t("gif", Kind::Image, true),
    t("bmp", Kind::Image, true),
    t("tiff", Kind::Image, true),
    t("ico", Kind::Image, true),
];
const ANIMATION_OUT: [Target; 4] = [
    t("mp4", Kind::Video, false),
    t("webm", Kind::Video, false),
    t("gif", Kind::Animation, false),
    t("png", Kind::Image, true),
];

/// Formats a file can become, most useful first.
pub fn targets_for(media: &Media) -> Vec<Target> {
    match media.kind {
        Some(Kind::Video) => {
            let audio = if media.has_audio { &AUDIO_OUT[..] } else { &[] };
            VIDEO_OUT.iter().chain(audio).copied().collect()
        }
        Some(Kind::Audio) => AUDIO_OUT.to_vec(),
        Some(Kind::Image) => IMAGE_OUT.to_vec(),
        Some(Kind::Animation) => ANIMATION_OUT.to_vec(),
        None => Vec::new(),
    }
}

pub fn find_target(media: &Media, id: &str) -> Option<Target> {
    targets_for(media).into_iter().find(|t| t.id == id)
}

/// H.264 encoders in the order Duri tries them: the GPU's own first, then
/// Windows' Media Foundation encoder, then OpenH264. The LGPL ffmpeg build
/// has no x264 (GPL).
pub const H264_ENCODERS: [&str; 5] = ["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "libopenh264"];

/// The software H.264 encoder, for when the GPU's can't (see `h264_for`).
pub const SOFTWARE_H264: &str = "libopenh264";

/// GPU encoders refuse very small frames (NVENC's minimum is about 145 px
/// wide); software is quick at that size anyway.
pub fn h264_for<'a>(media: &Media, adv: &Advanced, detected: &'a str) -> &'a str {
    let (w, h) = scaled(media, adv.max_height);
    if w < 256 || h < 144 { SOFTWARE_H264 } else { detected }
}

/// Everything ffmpeg needs besides the program name.
pub struct Job<'a> {
    pub input: &'a str,
    pub output: &'a str,
    pub media: &'a Media,
    pub target: Target,
    pub quality: Quality,
    pub advanced: &'a Advanced,
    /// The H.264 encoder that works on this PC (see `H264_ENCODERS`).
    pub h264: &'a str,
}

fn s(v: impl ToString) -> String {
    v.to_string()
}

/// Seconds as ffmpeg reads them ("12.5").
fn secs(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    format!("{r}")
}

/// How long the output will be, for the progress bar.
pub fn output_duration(media: &Media, adv: &Advanced) -> Option<f64> {
    let start = adv.start.unwrap_or(0.0).max(0.0);
    let end = match (adv.end, media.duration) {
        (Some(e), Some(d)) => e.min(d),
        (Some(e), None) => e,
        (None, d) => d?,
    };
    (end > start).then_some(end - start)
}

pub fn build_args(job: &Job) -> Vec<String> {
    let Job { input, output, media, target, quality, advanced: adv, .. } = *job;
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]
        .map(s)
        .to_vec();
    let timed = matches!(media.kind, Some(Kind::Video | Kind::Audio | Kind::Animation));
    if timed && let Some(start) = adv.start.filter(|s| *s > 0.0) {
        a.extend([s("-ss"), secs(start)]);
    }
    a.extend([s("-i"), s(input)]);
    if timed && let Some(end) = adv.end {
        let len = end - adv.start.unwrap_or(0.0).max(0.0);
        if len > 0.0 {
            a.extend([s("-t"), secs(len)]);
        }
    }

    match target.output {
        Kind::Audio => audio_args(&mut a, target.id, quality, adv),
        Kind::Video => video_args(&mut a, job),
        Kind::Animation => gif_args(&mut a, quality, adv, true),
        Kind::Image => image_args(&mut a, media, target.id, quality, adv),
    }
    a.push(s(output));
    a
}

fn audio_args(a: &mut Vec<String>, id: &str, q: Quality, adv: &Advanced) {
    a.extend(["-map", "0:a:0", "-vn", "-map_metadata", "0"].map(s));
    let bitrate = |default: u32| s(format!("{}k", adv.audio_bitrate.unwrap_or(default)));
    match id {
        "mp3" => {
            a.extend([s("-c:a"), s("libmp3lame")]);
            match adv.audio_bitrate {
                Some(b) => a.extend([s("-b:a"), s(format!("{b}k"))]),
                None => a.extend([s("-q:a"), s(q.pick(0, 2, 5))]),
            }
        }
        "m4a" => a.extend([s("-c:a"), s("aac"), s("-b:a"), bitrate(q.pick(256, 192, 128))]),
        "ogg" => {
            a.extend([s("-c:a"), s("libvorbis")]);
            match adv.audio_bitrate {
                Some(b) => a.extend([s("-b:a"), s(format!("{b}k"))]),
                None => a.extend([s("-q:a"), s(q.pick(7, 5, 3))]),
            }
        }
        "opus" => a.extend([s("-c:a"), s("libopus"), s("-b:a"), bitrate(q.pick(160, 128, 80))]),
        "flac" => a.extend(["-c:a", "flac", "-compression_level", "8"].map(s)),
        "wav" => a.extend(["-c:a", "pcm_s16le"].map(s)),
        "aiff" => a.extend(["-c:a", "pcm_s16be"].map(s)),
        _ => {}
    }
    audio_shape(a, adv);
}

fn audio_shape(a: &mut Vec<String>, adv: &Advanced) {
    if let Some(rate) = adv.sample_rate {
        a.extend([s("-ar"), s(rate)]);
    }
    if adv.mono {
        a.extend([s("-ac"), s("1")]);
    }
}

/// Containers that can take the source's streams as they are.
fn copyable(container: &str, media: &Media) -> bool {
    let v = media.video_codec.as_deref().unwrap_or("");
    let au = media.audio_codec.as_deref();
    let (videos, audios): (&[&str], &[&str]) = match container {
        "mp4" => (&["h264", "hevc", "av1", "mpeg4"], &["aac", "mp3", "opus", "ac3", "eac3", "flac", "alac"]),
        "mov" => (&["h264", "hevc", "prores", "mpeg4", "mjpeg"], &["aac", "mp3", "alac", "pcm_s16le", "pcm_s24le"]),
        "mkv" => return !v.is_empty(),
        "webm" => (&["vp8", "vp9", "av1"], &["opus", "vorbis"]),
        _ => return false,
    };
    videos.contains(&v) && au.is_none_or(|c| audios.contains(&c))
}

/// "High" without size, frame rate or trim changes keeps the original
/// streams when the new container can hold them: lossless and instant.
pub fn stream_copy(job: &Job) -> bool {
    let adv = job.advanced;
    job.quality == Quality::High
        && job.media.kind == Some(Kind::Video)
        && adv.max_height.is_none()
        && adv.fps.is_none()
        && !adv.trims()
        && adv.audio_bitrate.is_none()
        && adv.sample_rate.is_none()
        && !adv.mono
        && copyable(job.target.id, job.media)
}

/// Output size after "never upscale" limits, rounded to even numbers.
fn scaled(media: &Media, max_height: Option<u32>) -> (u32, u32) {
    let (w, h) = (media.width.unwrap_or(1280), media.height.unwrap_or(720));
    let (w, h) = match max_height {
        Some(m) if h > m => ((w as f64 * m as f64 / h as f64).round() as u32, m),
        _ => (w, h),
    };
    (w & !1, h & !1)
}

/// kbps for H.264 from bits per pixel per frame, kept within sane limits.
pub fn h264_kbps(media: &Media, q: Quality, adv: &Advanced) -> u32 {
    let (w, h) = scaled(media, adv.max_height);
    let fps = adv.fps.or(media.fps).unwrap_or(30.0).clamp(1.0, 120.0);
    let bpp = q.pick(0.14, 0.085, 0.045);
    let kbps = w as f64 * h as f64 * fps * bpp / 1000.0;
    (kbps.round() as u32).clamp(250, 60_000)
}

fn video_args(a: &mut Vec<String>, job: &Job) {
    let Job { media, target, quality: q, advanced: adv, h264, .. } = *job;
    let audio = media.has_audio && !adv.mute;
    if stream_copy(job) {
        a.extend(["-map", "0:v:0"].map(s));
        if audio {
            a.extend(["-map", "0:a?"].map(s));
        }
        a.extend(["-c", "copy", "-map_metadata", "0"].map(s));
        if !audio {
            a.push(s("-an"));
        }
        if matches!(target.id, "mp4" | "mov") {
            a.extend(["-movflags", "+faststart"].map(s));
        }
        return;
    }

    a.extend(["-map", "0:v:0"].map(s));
    if audio {
        a.extend(["-map", "0:a:0?"].map(s));
    }
    a.extend(["-map_metadata", "0"].map(s));

    // Even dimensions (H.264 and VP9 with 4:2:0 need them), never upscaled.
    let mut filters = vec![match adv.max_height {
        Some(m) => format!("scale=-2:'trunc(min({m},ih)/2)*2'"),
        None => s("scale='trunc(iw/2)*2':'trunc(ih/2)*2'"),
    }];
    if let Some(fps) = adv.fps {
        filters.push(format!("fps={}", secs(fps)));
    }
    a.extend([s("-vf"), filters.join(",")]);

    if target.id == "webm" {
        a.extend(["-c:v", "libvpx-vp9", "-b:v", "0", "-crf"].map(s));
        a.push(s(q.pick(28, 33, 40)));
        a.extend(["-row-mt", "1", "-deadline", "good", "-cpu-used", "4", "-pix_fmt", "yuv420p"].map(s));
        if audio {
            a.extend([s("-c:a"), s("libopus"), s("-b:a"), s(format!("{}k", adv.audio_bitrate.unwrap_or(q.pick(160, 128, 96))))]);
        }
    } else {
        let kbps = h264_kbps(media, q, adv);
        let h264 = h264_for(media, adv, h264);
        a.extend([s("-c:v"), s(h264)]);
        match h264 {
            "h264_nvenc" => a.extend(["-preset", "p5", "-rc", "vbr"].map(s)),
            "h264_qsv" => a.extend(["-preset", "medium"].map(s)),
            "h264_amf" => a.extend(["-quality", "balanced", "-rc", "vbr_peak"].map(s)),
            _ => {}
        }
        a.extend([
            s("-b:v"),
            s(format!("{kbps}k")),
            s("-maxrate"),
            s(format!("{}k", kbps * 3 / 2)),
            s("-bufsize"),
            s(format!("{}k", kbps * 2)),
        ]);
        let pix = if matches!(h264, "h264_qsv" | "h264_mf") { "nv12" } else { "yuv420p" };
        a.extend([s("-pix_fmt"), s(pix)]);
        if audio {
            a.extend([s("-c:a"), s("aac"), s("-b:a"), s(format!("{}k", adv.audio_bitrate.unwrap_or(q.pick(192, 160, 128))))]);
        }
        if matches!(target.id, "mp4" | "mov") {
            a.extend(["-movflags", "+faststart"].map(s));
        }
    }
    if audio {
        audio_shape(a, adv);
    } else {
        a.push(s("-an"));
    }
    a.push(s("-sn"));
}

/// GIF with its own palette: far better colors than ffmpeg's default.
fn gif_args(a: &mut Vec<String>, q: Quality, adv: &Advanced, animated: bool) {
    let mut chain = Vec::new();
    if animated {
        chain.push(format!("fps={}", secs(adv.fps.unwrap_or(q.pick(20.0, 15.0, 10.0)))));
    }
    let width = adv.max_width.or(animated.then(|| q.pick(640, 480, 360)));
    if let Some(w) = width {
        chain.push(format!("scale='min({w},iw)':-1:flags=lanczos"));
    }
    let dither = q.pick("sierra2_4a", "sierra2_4a", "bayer:bayer_scale=3");
    let stats = if animated { "=stats_mode=diff" } else { "" };
    let head = if chain.is_empty() { s("[0:v]") } else { format!("[0:v]{},", chain.join(",")) };
    let graph = format!("{head}split[a][b];[a]palettegen{stats}[p];[b][p]paletteuse=dither={dither}");
    a.extend([s("-filter_complex"), graph, s("-an")]);
    if animated {
        a.extend(["-loop", "0"].map(s));
    } else {
        a.extend(["-frames:v", "1"].map(s));
    }
}

fn image_args(a: &mut Vec<String>, media: &Media, id: &str, q: Quality, adv: &Advanced) {
    if id == "gif" {
        gif_args(a, q, adv, false);
        return;
    }
    let mut chain = Vec::new();
    if let Some(w) = adv.max_width {
        chain.push(format!("scale='min({w},iw)':-1:flags=lanczos"));
    }
    if id == "ico" {
        chain.push(s("scale='min(256,iw)':'min(256,ih)':force_original_aspect_ratio=decrease"));
    }
    // JPG and AVIF (as written here) have no transparency: put it on white
    // instead of the black ffmpeg would leave.
    let flatten = media.alpha && matches!(id, "jpg" | "avif");
    if flatten {
        let base = if chain.is_empty() { s("[0:v]") } else { format!("[0:v]{},", chain.join(",")) };
        let graph = format!(
            "{base}format=rgba,split[fg][size];[size]drawbox=c=white:t=fill[bg];[bg][fg]overlay=format=auto,format=rgb24"
        );
        a.extend([s("-filter_complex"), graph]);
    } else if !chain.is_empty() {
        a.extend([s("-vf"), chain.join(",")]);
    }
    a.extend(["-map_metadata", "0", "-an", "-frames:v", "1", "-update", "1"].map(s));
    match id {
        "jpg" => a.extend([s("-c:v"), s("mjpeg"), s("-q:v"), s(q.pick(2, 4, 7))]),
        "webp" => a.extend([s("-c:v"), s("libwebp"), s("-quality"), s(q.pick(92, 80, 65))]),
        "avif" => {
            a.extend(["-c:v", "libaom-av1", "-still-picture", "1", "-cpu-used", "6", "-row-mt", "1", "-crf"].map(s));
            a.push(s(q.pick(20, 30, 40)));
            a.extend(["-pix_fmt", "yuv420p"].map(s));
        }
        "png" => a.extend(["-c:v", "png"].map(s)),
        "bmp" => a.extend(["-c:v", "bmp"].map(s)),
        "tiff" => a.extend(["-c:v", "tiff", "-compression_algo", "lzw"].map(s)),
        "ico" => a.extend(["-c:v", "png"].map(s)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video() -> Media {
        Media {
            kind: Some(Kind::Video),
            duration: Some(60.0),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            has_audio: true,
            alpha: false,
        }
    }

    fn args(media: &Media, target: &str, quality: Quality, adv: &Advanced) -> String {
        let target = find_target(media, target).expect("target");
        build_args(&Job { input: "in", output: "out", media, target, quality, advanced: adv, h264: "h264_mf" }).join(" ")
    }

    #[test]
    fn targets_follow_the_kind() {
        let ids = |m: &Media| targets_for(m).iter().map(|t| t.id).collect::<Vec<_>>().join(",");
        assert_eq!(ids(&video()), "mp4,mkv,webm,mov,gif,mp3,m4a,wav,flac,ogg,opus,aiff");
        let silent = Media { has_audio: false, audio_codec: None, ..video() };
        assert_eq!(ids(&silent), "mp4,mkv,webm,mov,gif");
        assert_eq!(ids(&Media { kind: Some(Kind::Image), ..Default::default() }), "png,jpg,webp,avif,gif,bmp,tiff,ico");
        assert!(targets_for(&Media::default()).is_empty());
    }

    #[test]
    fn high_quality_remux_keeps_streams() {
        let a = args(&video(), "mkv", Quality::High, &Advanced::default());
        assert!(a.contains("-c copy"), "{a}");
        // Balanced re-encodes; so does anything that trims.
        assert!(!args(&video(), "mkv", Quality::Balanced, &Advanced::default()).contains("-c copy"));
        let trim = Advanced { start: Some(5.0), ..Default::default() };
        assert!(!args(&video(), "mp4", Quality::High, &trim).contains("-c copy"));
        // WebM can't hold H.264.
        assert!(args(&video(), "webm", Quality::High, &Advanced::default()).contains("libvpx-vp9"));
    }

    #[test]
    fn h264_bitrate_scales_with_size_and_quality() {
        let m = video();
        let d = Advanced::default();
        assert_eq!(h264_kbps(&m, Quality::Balanced, &d), 5288);
        assert!(h264_kbps(&m, Quality::Small, &d) < h264_kbps(&m, Quality::Balanced, &d));
        let p720 = Advanced { max_height: Some(720), ..Default::default() };
        assert_eq!(h264_kbps(&m, Quality::Balanced, &p720), 2350);
        // Never upscaled: a 4K limit on 1080p changes nothing.
        let p2160 = Advanced { max_height: Some(2160), ..Default::default() };
        assert_eq!(h264_kbps(&m, Quality::Balanced, &p2160), 5288);
    }

    #[test]
    fn video_reencode_args() {
        let adv = Advanced { max_height: Some(720), fps: Some(30.0), mute: true, ..Default::default() };
        let a = args(&video(), "mp4", Quality::Balanced, &adv);
        assert!(a.contains("-vf scale=-2:'trunc(min(720,ih)/2)*2',fps=30"), "{a}");
        assert!(a.contains("-c:v h264_mf -b:v 2350k -maxrate 3525k"), "{a}");
        assert!(a.contains("-an") && !a.contains("-c:a"), "{a}");
        assert!(a.ends_with("-movflags +faststart -an -sn out"), "{a}");
    }

    #[test]
    fn tiny_frames_use_the_software_encoder() {
        let gif = Media { kind: Some(Kind::Animation), width: Some(120), height: Some(80), ..Default::default() };
        assert_eq!(h264_for(&gif, &Advanced::default(), "h264_nvenc"), SOFTWARE_H264);
        assert_eq!(h264_for(&video(), &Advanced::default(), "h264_nvenc"), "h264_nvenc");
        let p360 = Advanced { max_height: Some(120), ..Default::default() };
        assert_eq!(h264_for(&video(), &p360, "h264_nvenc"), SOFTWARE_H264);
    }

    #[test]
    fn trim_uses_seek_and_length() {
        let adv = Advanced { start: Some(10.0), end: Some(25.5), ..Default::default() };
        let a = args(&video(), "mp3", Quality::Balanced, &adv);
        assert!(a.contains("-ss 10 -i in -t 15.5"), "{a}");
        assert_eq!(output_duration(&video(), &adv), Some(15.5));
        assert_eq!(output_duration(&video(), &Advanced::default()), Some(60.0));
        let past_end = Advanced { end: Some(90.0), ..Default::default() };
        assert_eq!(output_duration(&video(), &past_end), Some(60.0));
    }

    #[test]
    fn audio_presets_and_overrides() {
        let m = Media { kind: Some(Kind::Audio), has_audio: true, ..Default::default() };
        assert!(args(&m, "mp3", Quality::High, &Advanced::default()).contains("-c:a libmp3lame -q:a 0"));
        let adv = Advanced { audio_bitrate: Some(320), sample_rate: Some(44100), mono: true, ..Default::default() };
        let a = args(&m, "mp3", Quality::High, &adv);
        assert!(a.contains("-b:a 320k -ar 44100 -ac 1"), "{a}");
        assert!(args(&m, "m4a", Quality::Small, &Advanced::default()).contains("-c:a aac -b:a 128k"));
        assert!(args(&m, "flac", Quality::Small, &Advanced::default()).contains("-c:a flac"));
    }

    #[test]
    fn gif_from_video_uses_a_palette() {
        let a = args(&video(), "gif", Quality::Balanced, &Advanced::default());
        assert!(
            a.contains("[0:v]fps=15,scale='min(480,iw)':-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a"),
            "{a}"
        );
        assert!(a.contains("-loop 0"));
    }

    #[test]
    fn jpg_from_transparent_png_goes_on_white() {
        let m = Media { kind: Some(Kind::Image), alpha: true, width: Some(10), height: Some(10), ..Default::default() };
        let a = args(&m, "jpg", Quality::Balanced, &Advanced::default());
        assert!(a.contains("drawbox=c=white:t=fill") && a.contains("-c:v mjpeg -q:v 4"), "{a}");
        let opaque = Media { alpha: false, ..m };
        assert!(!args(&opaque, "jpg", Quality::Balanced, &Advanced::default()).contains("drawbox"));
    }

    #[test]
    fn ico_fits_in_256() {
        let m = Media { kind: Some(Kind::Image), ..Default::default() };
        let a = args(&m, "ico", Quality::Balanced, &Advanced::default());
        assert!(a.contains("force_original_aspect_ratio=decrease") && a.contains("-c:v png"), "{a}");
    }
}
