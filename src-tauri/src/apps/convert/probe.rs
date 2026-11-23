//! Reads `ffprobe -print_format json -show_format -show_streams` output.
//! The file's contents decide what it is, not its extension.

use super::formats::{Kind, Media};
use serde_json::Value;

/// Pixel formats with transparency. `pal8` may or may not have it (PNG with
/// tRNS, GIF); flattening an opaque one on white changes nothing.
fn has_alpha(pix_fmt: &str) -> bool {
    pix_fmt == "pal8"
        || pix_fmt.starts_with("yuva")
        || pix_fmt.starts_with("rgba")
        || pix_fmt.starts_with("bgra")
        || pix_fmt.starts_with("argb")
        || pix_fmt.starts_with("abgr")
        || pix_fmt.starts_with("ya")
        || pix_fmt.starts_with("gbrap")
}

/// Formats ffmpeg reads as still pictures.
fn still_format(format: &str) -> bool {
    format.ends_with("_pipe") || matches!(format, "image2" | "ico" | "tiff" | "bmp" | "png")
}

/// "30000/1001" → 29.97
fn rate(text: &str) -> Option<f64> {
    let (n, d) = text.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    let fps = n / d;
    (d > 0.0 && fps > 0.0 && fps <= 240.0).then_some(fps)
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// Whether a GIF or APNG needs its frames counted (see `parse`).
pub fn needs_frame_count(json: &str) -> bool {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v["format"]["format_name"].as_str().map(|f| matches!(f, "gif" | "apng")))
        .unwrap_or(false)
}

/// `frames` is the frame count of a GIF or APNG (from a second ffprobe run
/// with `-count_packets`): more than one makes it an animation.
pub fn parse(json: &str, frames: Option<u64>) -> Media {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Media::default();
    };
    let format = v["format"]["format_name"].as_str().unwrap_or("");
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let is = |s: &Value, kind: &str| s["codec_type"].as_str() == Some(kind);
    let cover = |s: &Value| s["disposition"]["attached_pic"].as_i64() == Some(1);
    let picture = streams.iter().find(|s| is(s, "video") && !cover(s));
    let audio = streams.iter().find(|s| is(s, "audio"));

    let mut media = Media {
        duration: number(&v["format"]["duration"]).filter(|d| *d > 0.0),
        has_audio: audio.is_some(),
        audio_codec: audio.and_then(|s| s["codec_name"].as_str()).map(String::from),
        ..Media::default()
    };
    if let Some(p) = picture {
        media.video_codec = p["codec_name"].as_str().map(String::from);
        media.width = p["width"].as_u64().map(|w| w as u32);
        media.height = p["height"].as_u64().map(|h| h as u32);
        media.fps = p["avg_frame_rate"].as_str().and_then(rate).or_else(|| p["r_frame_rate"].as_str().and_then(rate));
        media.alpha = p["pix_fmt"].as_str().is_some_and(has_alpha);
    }

    media.kind = match (picture, format) {
        (Some(_), "gif" | "apng") => Some(if frames.unwrap_or(1) > 1 { Kind::Animation } else { Kind::Image }),
        (Some(_), f) if still_format(f) => Some(Kind::Image),
        (Some(_), _) => Some(Kind::Video),
        (None, _) if media.has_audio => Some(Kind::Audio),
        _ => None,
    };
    if matches!(media.kind, Some(Kind::Image)) {
        media.duration = None;
        media.fps = None;
    }
    media
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_with_sound() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001","pix_fmt":"yuv420p","disposition":{"attached_pic":0}},
            {"codec_type":"audio","codec_name":"aac"}],
            "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.480000"}}"#;
        let m = parse(json, None);
        assert_eq!(m.kind, Some(Kind::Video));
        assert_eq!((m.width, m.height), (Some(1920), Some(1080)));
        assert!((m.fps.unwrap() - 29.97).abs() < 0.01);
        assert_eq!(m.duration, Some(12.48));
        assert_eq!(m.video_codec.as_deref(), Some("h264"));
        assert_eq!(m.audio_codec.as_deref(), Some("aac"));
        assert!(m.has_audio && !m.alpha);
    }

    #[test]
    fn song_with_cover_art_is_audio() {
        let json = r#"{"streams":[
            {"codec_type":"audio","codec_name":"mp3"},
            {"codec_type":"video","codec_name":"mjpeg","width":600,"height":600,"disposition":{"attached_pic":1}}],
            "format":{"format_name":"mp3","duration":"200.1"}}"#;
        let m = parse(json, None);
        assert_eq!(m.kind, Some(Kind::Audio));
        assert_eq!(m.width, None);
    }

    #[test]
    fn pictures() {
        let png = r#"{"streams":[{"codec_type":"video","codec_name":"png","width":64,"height":32,"pix_fmt":"rgba"}],
            "format":{"format_name":"png_pipe"}}"#;
        let m = parse(png, None);
        assert_eq!(m.kind, Some(Kind::Image));
        assert!(m.alpha);
        assert_eq!(m.duration, None);
        let jpg = r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","width":64,"height":32,"pix_fmt":"yuvj420p"}],
            "format":{"format_name":"jpeg_pipe","duration":"0.04"}}"#;
        let m = parse(jpg, None);
        assert_eq!(m.kind, Some(Kind::Image));
        assert!(!m.alpha);
    }

    #[test]
    fn gif_frames_decide() {
        let gif = r#"{"streams":[{"codec_type":"video","codec_name":"gif","width":100,"height":100,"pix_fmt":"bgra"}],
            "format":{"format_name":"gif","duration":"2.5"}}"#;
        assert!(needs_frame_count(gif));
        assert_eq!(parse(gif, Some(25)).kind, Some(Kind::Animation));
        assert_eq!(parse(gif, Some(1)).kind, Some(Kind::Image));
    }

    #[test]
    fn not_media() {
        assert_eq!(parse("not json", None).kind, None);
        assert_eq!(parse(r#"{"streams":[],"format":{"format_name":"tty"}}"#, None).kind, None);
    }
}
