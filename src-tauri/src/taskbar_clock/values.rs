//! The values behind the template keys: one [`Snapshot`] per tick, sampled
//! by the worker and formatted here.

use super::template::Key;
use super::weather::{self, Weather};

/// Shown for a value that isn't there (no battery, no city picked yet).
const MISSING: &str = "–";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub cpu: u8,
    pub ram: u8,
    pub ram_used_gb: f64,
    pub gpu: u8,
    pub battery: Option<u8>,
    /// Bytes per second.
    pub down: f64,
    pub up: f64,
    pub disk_read: f64,
    pub disk_write: f64,
    pub weather: Option<Weather>,
    pub city: String,
    pub song: String,
    pub artist: String,
    /// Weather conditions in Turkish rather than English.
    pub turkish: bool,
}

/// A transfer speed with a unit that keeps it short: "0 KB/s", "850 KB/s",
/// "1.2 MB/s".
pub fn rate(bytes_per_second: f64) -> String {
    const KB: f64 = 1024.0;
    let value = if bytes_per_second.is_finite() { bytes_per_second.max(0.0) } else { 0.0 };
    if value < 999.5 * KB {
        format!("{:.0} KB/s", value / KB)
    } else if value < 999.95 * KB * KB {
        format!("{:.1} MB/s", value / (KB * KB))
    } else {
        format!("{:.1} GB/s", value / (KB * KB * KB))
    }
}

impl Snapshot {
    pub fn value(&self, key: Key) -> String {
        let weather = |show: &dyn Fn(&Weather) -> String| self.weather.as_ref().map_or(MISSING.to_owned(), show);
        let today = |show: &dyn Fn(&weather::Day) -> String| {
            self.weather.as_ref().and_then(|w| w.days.first()).map_or(MISSING.to_owned(), show)
        };
        let or_missing = |text: &str| if text.is_empty() { MISSING.to_owned() } else { text.to_owned() };
        match key {
            Key::Cpu => self.cpu.to_string(),
            Key::Ram => self.ram.to_string(),
            Key::RamUsed => format!("{:.1} GB", self.ram_used_gb),
            Key::Gpu => self.gpu.to_string(),
            Key::Battery => self.battery.map_or(MISSING.to_owned(), |level| level.to_string()),
            Key::Down => rate(self.down),
            Key::Up => rate(self.up),
            Key::DiskRead => rate(self.disk_read),
            Key::DiskWrite => rate(self.disk_write),
            Key::Temp => weather(&|w| weather::degrees(w.temp)),
            Key::Feels => weather(&|w| weather::degrees(w.feels)),
            Key::Weather => weather(&|w| weather::condition(w.code, w.is_day, self.turkish).to_owned()),
            Key::Icon => weather(&|w| weather::icon(w.code, w.is_day).to_owned()),
            Key::Humidity => weather(&|w| w.humidity.to_string()),
            Key::Wind => weather(&|w| w.wind_text()),
            Key::High => today(&|day| weather::degrees(day.high)),
            Key::Low => today(&|day| weather::degrees(day.low)),
            Key::Rain => today(&|day| day.rain.to_string()),
            Key::City => or_missing(&self.city),
            Key::Song => or_missing(&self.song),
            Key::Artist => or_missing(&self.artist),
        }
    }

    pub fn forecast(&self, days: u8, day_name: &dyn Fn(&str) -> String) -> String {
        self.weather.as_ref().map_or(MISSING.to_owned(), |w| w.forecast(days, day_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_pick_a_short_unit() {
        assert_eq!(rate(0.0), "0 KB/s");
        assert_eq!(rate(f64::NAN), "0 KB/s");
        assert_eq!(rate(870_400.0), "850 KB/s");
        assert_eq!(rate(1023.0 * 1024.0), "1.0 MB/s");
        assert_eq!(rate(1.25 * 1024.0 * 1024.0), "1.2 MB/s");
        assert_eq!(rate(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB/s");
    }

    #[test]
    fn missing_values_show_a_dash() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.value(Key::Battery), "–");
        assert_eq!(snapshot.value(Key::Temp), "–");
        assert_eq!(snapshot.value(Key::High), "–");
        assert_eq!(snapshot.value(Key::Song), "–");
        assert_eq!(snapshot.forecast(3, &|_| String::new()), "–");
        assert_eq!(snapshot.value(Key::Cpu), "0");
        assert_eq!(snapshot.value(Key::RamUsed), "0.0 GB");
    }
}
