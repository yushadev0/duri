//! The little template language of the taskbar clock lines.
//!
//! `{time}` and `{date}` follow Windows' regional format; `{time:HH:mm:ss}`
//! and `{date:ddd d MMM}` take a Windows format picture. Every other token is
//! a [`Key`] (`{cpu}`, `{down}`, `{temp}`, ...) or `{forecast}`. Anything
//! else, unknown `{tokens}` included, is shown as typed.
//!
//! [`REFERENCE`] is the list Settings shows, with a live example for each
//! entry. A new token belongs in [`Key::NAMES`] (or `token`), in `REFERENCE`,
//! and in `settings.taskbarClock.tokens` of both language files.

/// A value Duri fills in, sampled by `values::Sampler`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Cpu,
    Ram,
    RamUsed,
    Gpu,
    Battery,
    Down,
    Up,
    DiskRead,
    DiskWrite,
    Temp,
    Feels,
    Weather,
    Icon,
    High,
    Low,
    Humidity,
    Wind,
    Rain,
    City,
    Song,
    Artist,
}

impl Key {
    const NAMES: &'static [(&'static str, Key)] = &[
        ("cpu", Key::Cpu),
        ("ram", Key::Ram),
        ("ram_used", Key::RamUsed),
        ("gpu", Key::Gpu),
        ("battery", Key::Battery),
        ("down", Key::Down),
        ("up", Key::Up),
        ("disk_read", Key::DiskRead),
        ("disk_write", Key::DiskWrite),
        ("temp", Key::Temp),
        ("feels", Key::Feels),
        ("weather", Key::Weather),
        ("icon", Key::Icon),
        ("high", Key::High),
        ("low", Key::Low),
        ("humidity", Key::Humidity),
        ("wind", Key::Wind),
        ("rain", Key::Rain),
        ("city", Key::City),
        ("song", Key::Song),
        ("artist", Key::Artist),
    ];

    /// Comes from the weather service.
    pub fn is_weather(self) -> bool {
        matches!(
            self,
            Key::Temp | Key::Feels | Key::Weather | Key::Icon | Key::High | Key::Low | Key::Humidity | Key::Wind | Key::Rain
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    /// `None` = the regional short time.
    Time(Option<String>),
    /// `None` = the regional short date.
    Date(Option<String>),
    /// The next 1 – 7 days.
    Forecast(u8),
    Value(Key),
}

pub const DEFAULT_FORECAST_DAYS: u8 = 3;
pub const MAX_FORECAST_DAYS: u8 = 7;

/// One row of the reference in Settings: an i18n id, its group, and what to
/// type. The example shown is this template rendered right now.
pub struct Reference {
    pub id: &'static str,
    pub group: &'static str,
    pub template: &'static str,
}

const fn reference(id: &'static str, group: &'static str, template: &'static str) -> Reference {
    Reference { id, group, template }
}

pub const REFERENCE: &[Reference] = &[
    reference("time", "time", "{time}"),
    reference("timeCustom", "time", "{time:HH:mm:ss}"),
    reference("date", "time", "{date}"),
    reference("dateCustom", "time", "{date:d MMMM}"),
    reference("weekday", "time", "{weekday}"),
    reference("weekdayShort", "time", "{weekday:short}"),
    reference("cpu", "system", "{cpu}"),
    reference("ram", "system", "{ram}"),
    reference("ramUsed", "system", "{ram_used}"),
    reference("gpu", "system", "{gpu}"),
    reference("battery", "system", "{battery}"),
    reference("down", "transfer", "{down}"),
    reference("up", "transfer", "{up}"),
    reference("diskRead", "transfer", "{disk_read}"),
    reference("diskWrite", "transfer", "{disk_write}"),
    reference("temp", "weather", "{temp}"),
    reference("feels", "weather", "{feels}"),
    reference("icon", "weather", "{icon}"),
    reference("weather", "weather", "{weather}"),
    reference("high", "weather", "{high}"),
    reference("low", "weather", "{low}"),
    reference("rain", "weather", "{rain}"),
    reference("humidity", "weather", "{humidity}"),
    reference("wind", "weather", "{wind}"),
    reference("city", "weather", "{city}"),
    reference("forecast", "weather", "{forecast}"),
    reference("forecastWeek", "weather", "{forecast:7}"),
    reference("song", "media", "{song}"),
    reference("artist", "media", "{artist}"),
];

fn token(body: &str) -> Option<Part> {
    let (name, argument) = match body.split_once(':') {
        Some((name, argument)) => (name, Some(argument)),
        None => (body, None),
    };
    let argument = argument.filter(|a| !a.trim().is_empty());
    let name = name.trim().to_ascii_lowercase();
    match (name.as_str(), argument) {
        ("time", picture) => Some(Part::Time(picture.map(str::to_owned))),
        ("date", picture) => Some(Part::Date(picture.map(str::to_owned))),
        ("weekday", None) => Some(Part::Date(Some("dddd".into()))),
        ("weekday", Some(form)) if form.trim().eq_ignore_ascii_case("short") => Some(Part::Date(Some("ddd".into()))),
        ("forecast", None) => Some(Part::Forecast(DEFAULT_FORECAST_DAYS)),
        ("forecast", Some(days)) => {
            let days: u8 = days.trim().parse().ok()?;
            (1..=MAX_FORECAST_DAYS).contains(&days).then_some(Part::Forecast(days))
        }
        (name, None) => Key::NAMES.iter().find(|(known, _)| *known == name).map(|(_, key)| Part::Value(*key)),
        _ => None,
    }
}

pub fn parse(template: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| token(&after[..close]).map(|part| (part, close))) {
            Some((part, close)) => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(part);
                rest = &after[close + 1..];
            }
            None => {
                text.push('{');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

/// Where the values come from; `picture` is a Windows format picture.
pub trait Source {
    fn time(&self, picture: Option<&str>) -> String;
    fn date(&self, picture: Option<&str>) -> String;
    fn value(&self, key: Key) -> String;
    fn forecast(&self, days: u8) -> String;
}

pub fn render(parts: &[Part], source: &dyn Source) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            Part::Text(text) => out.push_str(text),
            Part::Time(picture) => out.push_str(&source.time(picture.as_deref())),
            Part::Date(picture) => out.push_str(&source.date(picture.as_deref())),
            Part::Forecast(days) => out.push_str(&source.forecast(*days)),
            Part::Value(key) => out.push_str(&source.value(*key)),
        }
    }
    out
}

/// The settings lines as the text the taskbar shows. Blank lines are
/// dropped, so the filled ones stay vertically centered.
pub fn render_lines(lines: &[Vec<Part>], source: &dyn Source) -> String {
    lines
        .iter()
        .map(|parts| render(parts, source))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn uses(lines: &[Vec<Part>], key: Key) -> bool {
    lines.iter().flatten().any(|part| *part == Part::Value(key))
}

pub fn uses_weather(lines: &[Vec<Part>]) -> bool {
    lines.iter().flatten().any(|part| match part {
        Part::Forecast(_) => true,
        Part::Value(key) => key.is_weather(),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;
    impl Source for Fixed {
        fn time(&self, picture: Option<&str>) -> String {
            format!("T[{}]", picture.unwrap_or("default"))
        }
        fn date(&self, picture: Option<&str>) -> String {
            format!("D[{}]", picture.unwrap_or("default"))
        }
        fn value(&self, key: Key) -> String {
            match key {
                Key::Cpu => "12".into(),
                Key::Ram => "48".into(),
                Key::Gpu => "7".into(),
                other => format!("<{other:?}>"),
            }
        }
        fn forecast(&self, days: u8) -> String {
            format!("F{days}")
        }
    }

    fn show(template: &str) -> String {
        render(&parse(template), &Fixed)
    }

    #[test]
    fn plain_text_and_usage() {
        assert_eq!(show("hello"), "hello");
        assert_eq!(show("CPU {cpu}%  RAM {ram}%  GPU {gpu}%"), "CPU 12%  RAM 48%  GPU 7%");
        assert_eq!(show("{CPU}{ Ram }"), "1248");
        assert_eq!(show("{disk_read} {down}"), "<DiskRead> <Down>");
    }

    #[test]
    fn time_and_date_pictures() {
        assert_eq!(show("{time}"), "T[default]");
        assert_eq!(show("{time:HH:mm:ss} | {date:ddd d MMM}"), "T[HH:mm:ss] | D[ddd d MMM]");
        assert_eq!(show("{date:}"), "D[default]");
        assert_eq!(show("{weekday} {weekday:short} {weekday:SHORT}"), "D[dddd] D[ddd] D[ddd]");
    }

    #[test]
    fn forecast_days() {
        assert_eq!(show("{forecast} {forecast:7} {forecast: 1 }"), "F3 F7 F1");
        assert_eq!(show("{forecast:0} {forecast:8} {forecast:x}"), "{forecast:0} {forecast:8} {forecast:x}");
    }

    #[test]
    fn unknown_tokens_and_stray_braces_stay() {
        assert_eq!(show("{nope} {cpu:1} {weekday:long} {"), "{nope} {cpu:1} {weekday:long} {");
        assert_eq!(show("a } b { c"), "a } b { c");
        assert_eq!(show("{{cpu}}"), "{12}");
    }

    #[test]
    fn blank_lines_are_dropped() {
        let lines = vec![parse("{time}"), parse("  "), parse("x")];
        assert_eq!(render_lines(&lines, &Fixed), "T[default]\nx");
        assert_eq!(render_lines(&[parse(""), parse("")], &Fixed), "");
    }

    #[test]
    fn knows_what_it_needs() {
        let lines = vec![parse("{time}"), parse("{gpu}%")];
        assert!(uses(&lines, Key::Gpu));
        assert!(!uses(&lines, Key::Cpu));
        assert!(!uses_weather(&lines));
        assert!(uses_weather(&[parse("{forecast:2}")]));
        assert!(uses_weather(&[parse("{temp}")]));
        assert!(!uses_weather(&[parse("{city}")]));
    }

    #[test]
    fn every_reference_entry_is_a_token() {
        for entry in REFERENCE {
            let parts = parse(entry.template);
            assert!(
                parts.len() == 1 && !matches!(parts[0], Part::Text(_)),
                "{} does not parse as one token",
                entry.template
            );
        }
        for (name, key) in Key::NAMES {
            let template = format!("{{{name}}}");
            assert_eq!(parse(&template), vec![Part::Value(*key)]);
            assert!(REFERENCE.iter().any(|entry| entry.template == template), "{template} is not in REFERENCE");
        }
    }
}
