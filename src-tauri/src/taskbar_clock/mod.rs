//! Taskbar clock: replaces the text of the Windows tray clock with up to
//! three lines the user writes in Settings (custom text, time and date
//! formats, system usage, transfer speeds, weather, the playing song).
//!
//! This is the one feature that runs code inside explorer.exe: `inject`
//! loads `duri_clock.dll` (src-clock/clock.cpp) into the taskbar thread, and
//! the thread here renders the lines once a second and sends them over.
//! See docs/ARCHITECTURE.md › Taskbar clock.

#[cfg(windows)]
mod inject;
mod perf;
pub mod template;
mod values;
mod weather;

use crate::db::Db;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use template::{Key, Part};
use values::Snapshot;

const NS: &str = "taskbar_clock";
const GUARD_KEY: &str = "guard";
/// Explorer dying this many times right after we loaded into it switches the
/// feature off.
const MAX_STRIKES: u32 = 2;
pub const LINES: usize = 3;
/// While Settings shows the token reference, every value is sampled, not
/// only the ones the lines use. Each request extends this window.
const REFERENCE_WINDOW: Duration = Duration::from_secs(3);

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Off,
    Starting,
    /// Our text is in the taskbar.
    Active,
    /// No taskbar to load into (explorer is not running).
    NoTaskbar,
    /// Loaded, but this Windows build has no clock we recognize.
    NoClock,
    Failed,
    /// Switched off by the crash guard.
    Blocked,
}

/// What we remember across runs to notice explorer crashing because of us.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
struct Guard {
    strikes: u32,
    /// Explorer process we loaded into and have not yet seen survive.
    pending_explorer: Option<u32>,
}

impl Guard {
    /// Called before loading into `explorer`. Returns false when the feature
    /// must be switched off instead.
    fn before_inject(&mut self, explorer: u32) -> bool {
        // The explorer we loaded into last time is gone before it was
        // confirmed healthy: count it against us.
        if self.pending_explorer.is_some_and(|pending| pending != explorer) {
            self.strikes += 1;
        }
        if self.strikes >= MAX_STRIKES {
            self.pending_explorer = None;
            return false;
        }
        self.pending_explorer = Some(explorer);
        true
    }

    fn confirm(&mut self) {
        *self = Self::default();
    }
}

/// "left", "center" or "right" as the number the DLL takes.
fn align_code(align: &str) -> u32 {
    match align {
        "left" => 0,
        "center" => 1,
        _ => 2,
    }
}

/// Whether Duri's own language is Turkish (weather conditions follow it).
fn is_turkish(language: &str) -> bool {
    match language {
        "tr" => true,
        "en" => false,
        _ => system_language_is_turkish(),
    }
}

#[cfg(windows)]
fn system_language_is_turkish() -> bool {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    // Primary language id of Turkish.
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x1f }
}

#[cfg(not(windows))]
fn system_language_is_turkish() -> bool {
    false
}

#[derive(Clone, Debug, PartialEq)]
struct Config {
    enabled: bool,
    lines: [String; LINES],
    align: u32,
    city: String,
    place: Option<(f64, f64)>,
    fahrenheit: bool,
    turkish: bool,
}

impl Config {
    fn of(settings: &Settings) -> Self {
        Self {
            enabled: settings.taskbar_clock,
            lines: [
                settings.taskbar_clock_line1.clone(),
                settings.taskbar_clock_line2.clone(),
                settings.taskbar_clock_line3.clone(),
            ],
            align: align_code(&settings.taskbar_clock_align),
            city: settings.taskbar_clock_city.clone(),
            place: settings.taskbar_clock_latitude.zip(settings.taskbar_clock_longitude),
            fahrenheit: settings.taskbar_clock_fahrenheit,
            turkish: is_turkish(&settings.language),
        }
    }

    fn parsed(&self) -> Vec<Vec<Part>> {
        self.lines.iter().map(|line| template::parse(line)).collect()
    }
}

/// The forecast for one place and unit, shared with the fetch thread.
#[derive(Default)]
struct WeatherCache {
    /// Latitude, longitude (in 1/10000°) and unit the data is for.
    key: Option<(i64, i64, bool)>,
    data: Option<weather::Weather>,
    /// When the last fetch finished, and whether it worked.
    fetched: Option<(Instant, bool)>,
    fetching: bool,
}

pub struct TaskbarClock {
    status: Mutex<Status>,
    snapshot: Mutex<Snapshot>,
    reference_until: Mutex<Option<Instant>>,
    weather: Arc<Mutex<WeatherCache>>,
    config: Mutex<Sender<Config>>,
}

impl TaskbarClock {
    fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// A snapshot plus the Windows regional formats: what the lines render from.
struct Live<'a>(&'a Snapshot);

impl template::Source for Live<'_> {
    fn time(&self, picture: Option<&str>) -> String {
        regional::time(picture)
    }
    fn date(&self, picture: Option<&str>) -> String {
        regional::date(picture)
    }
    fn value(&self, key: Key) -> String {
        self.0.value(key)
    }
    fn forecast(&self, days: u8) -> String {
        self.0.forecast(days, &regional::short_day)
    }
}

#[tauri::command]
pub fn taskbar_clock_status(state: State<'_, TaskbarClock>) -> Status {
    state.status.lock().map(|s| *s).unwrap_or(Status::Off)
}

/// The lines as the taskbar would show them right now (Settings preview).
#[tauri::command]
pub fn taskbar_clock_preview(state: State<'_, TaskbarClock>, lines: Vec<String>) -> String {
    let lines: Vec<_> = lines.iter().take(LINES).map(|line| template::parse(line)).collect();
    template::render_lines(&lines, &Live(&state.snapshot()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceRow {
    id: &'static str,
    group: &'static str,
    template: &'static str,
    /// What the template shows right now.
    example: String,
}

/// Every token with a live example, for the list in Settings. Asking keeps
/// all values sampled for the next few seconds.
#[tauri::command]
pub fn taskbar_clock_reference(state: State<'_, TaskbarClock>) -> Vec<ReferenceRow> {
    if let Ok(mut until) = state.reference_until.lock() {
        *until = Some(Instant::now() + REFERENCE_WINDOW);
    }
    let snapshot = state.snapshot();
    template::REFERENCE
        .iter()
        .map(|entry| ReferenceRow {
            id: entry.id,
            group: entry.group,
            template: entry.template,
            example: template::render(&template::parse(entry.template), &Live(&snapshot)),
        })
        .collect()
}

/// Cities matching `query`, for the weather picker.
#[tauri::command]
pub async fn taskbar_clock_search_city(query: String, language: String) -> Result<Vec<weather::Place>, String> {
    if query.trim().chars().count() < 2 {
        return Ok(Vec::new());
    }
    let url = weather::search_url(&query, is_turkish(&language));
    tauri::async_runtime::spawn_blocking(move || weather::parse_search(&crate::http::get(&url)?))
        .await
        .map_err(|e| e.to_string())?
}

/// Settings were saved: pass the new lines on, and forgive past crashes when
/// the user switches the feature back on.
pub fn apply(app: &AppHandle, db: &Db, previous: &Settings, settings: &Settings) {
    if settings.taskbar_clock && !previous.taskbar_clock {
        let _ = db.set_json(NS, GUARD_KEY, &Guard::default());
    }
    if let Some(state) = app.try_state::<TaskbarClock>()
        && let Ok(sender) = state.config.lock()
    {
        let _ = sender.send(Config::of(settings));
    }
}

pub fn init(app: &AppHandle, db: Arc<Db>, dir: &Path) {
    let (sender, receiver) = channel();
    app.manage(TaskbarClock {
        status: Mutex::new(Status::Off),
        snapshot: Mutex::new(Snapshot::default()),
        reference_until: Mutex::new(None),
        weather: Arc::new(Mutex::new(WeatherCache::default())),
        config: Mutex::new(sender),
    });
    #[cfg(windows)]
    {
        let initial = Config::of(&crate::settings::load(&db));
        let (app, dir) = (app.clone(), dir.to_path_buf());
        std::thread::Builder::new()
            .name("duri-taskbar-clock".into())
            .spawn(move || worker::run(app, db, dir, receiver, initial))
            .expect("failed to spawn taskbar clock thread");
    }
    #[cfg(not(windows))]
    {
        let _ = (db, dir, receiver);
    }
}

/// Time and date in the user's Windows regional format.
#[cfg(windows)]
mod regional {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::Globalization::{
        GetDateFormatEx, GetTimeFormatEx, DATE_SHORTDATE, ENUM_DATE_FORMATS_FLAGS, TIME_FORMAT_FLAGS, TIME_NOSECONDS,
    };

    fn text(buffer: &[u16], length: i32) -> String {
        String::from_utf16_lossy(&buffer[..(length.max(1) - 1) as usize])
    }

    pub fn time(picture: Option<&str>) -> String {
        let mut buffer = [0u16; 128];
        // A null locale is the user's; a null time is now.
        let length = unsafe {
            match picture {
                Some(picture) => GetTimeFormatEx(
                    PCWSTR::null(),
                    TIME_FORMAT_FLAGS(0),
                    None,
                    &HSTRING::from(picture),
                    Some(&mut buffer),
                ),
                None => GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, None, PCWSTR::null(), Some(&mut buffer)),
            }
        };
        text(&buffer, length)
    }

    fn date_of(day: Option<&SYSTEMTIME>, picture: Option<&str>) -> String {
        let mut buffer = [0u16; 128];
        let day = day.map(std::ptr::from_ref);
        let length = unsafe {
            match picture {
                Some(picture) => GetDateFormatEx(
                    PCWSTR::null(),
                    ENUM_DATE_FORMATS_FLAGS(0),
                    day,
                    &HSTRING::from(picture),
                    Some(&mut buffer),
                    PCWSTR::null(),
                ),
                None => {
                    GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, day, PCWSTR::null(), Some(&mut buffer), PCWSTR::null())
                }
            }
        };
        text(&buffer, length)
    }

    pub fn date(picture: Option<&str>) -> String {
        date_of(None, picture)
    }

    /// "2026-11-13" → "Fri" in the user's language.
    pub fn short_day(date: &str) -> String {
        let mut fields = date.split('-').map(|field| field.parse::<u16>().unwrap_or(0));
        let day = SYSTEMTIME {
            wYear: fields.next().unwrap_or(0),
            wMonth: fields.next().unwrap_or(0),
            wDay: fields.next().unwrap_or(0),
            ..Default::default()
        };
        date_of(Some(&day), Some("ddd"))
    }
}

#[cfg(not(windows))]
mod regional {
    pub fn time(_: Option<&str>) -> String {
        String::new()
    }
    pub fn date(_: Option<&str>) -> String {
        String::new()
    }
    pub fn short_day(_: &str) -> String {
        String::new()
    }
}

#[cfg(windows)]
mod worker {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::Emitter;

    /// How long explorer must survive after we load into it to be trusted.
    const CONFIRM_AFTER: Duration = Duration::from_secs(30);
    const RETRY_AFTER: Duration = Duration::from_secs(5);

    /// A performance counter that is open only while something shows it.
    #[derive(Default)]
    struct Lazy(Option<perf::Counter>);

    impl Lazy {
        fn counter(&mut self, wanted: bool, path: &str) -> Option<&mut perf::Counter> {
            if !wanted {
                self.0 = None;
            } else if self.0.is_none() {
                self.0 = perf::Counter::open(path);
            }
            self.0.as_mut()
        }
    }

    #[derive(Default)]
    struct Counters {
        gpu: Lazy,
        down: Lazy,
        up: Lazy,
        disk_read: Lazy,
        disk_write: Lazy,
    }

    struct Worker {
        app: AppHandle,
        db: Arc<Db>,
        dir: PathBuf,
        injector: Option<inject::Injector>,
        counters: Counters,
        cpu_times: Option<(u64, u64, u64)>,
        /// Window handle, alignment and text of what the DLL last took.
        shown: Option<(isize, u32, String)>,
        /// The last send found no clock; keep trying, it may not be built yet.
        no_clock: bool,
        last_attempt: Option<Instant>,
        /// When we loaded into explorer, until it is confirmed healthy.
        unconfirmed: Option<(Instant, u32)>,
        /// Whether a watch left open by an earlier run was picked up.
        pending_checked: bool,
        blocked: bool,
        /// Bumped when the lines change, so the DLL forgets the width it kept.
        generation: u32,
    }

    fn until_next_second() -> Duration {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Duration::from_millis(1005 - u64::from(now.subsec_millis()))
    }

    fn battery() -> Option<u8> {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut status).ok()? };
        // 255 = unknown, which is what a desktop reports.
        (status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent)
    }

    impl Worker {
        fn state(&self) -> State<'_, TaskbarClock> {
            self.app.state::<TaskbarClock>()
        }

        fn status(&self, status: Status) {
            let state = self.state();
            let changed = state.status.lock().map(|mut s| std::mem::replace(&mut *s, status) != status).unwrap_or(false);
            if changed {
                let _ = self.app.emit("taskbar-clock://status", status);
            }
        }

        fn guard(&self) -> Guard {
            self.db.get_json(NS, GUARD_KEY).unwrap_or_default()
        }

        /// The DLL's window, loading the DLL into explorer first if needed.
        fn host(&mut self) -> Option<windows::Win32::Foundation::HWND> {
            if self.injector.is_none() {
                match inject::Injector::new(&self.dir) {
                    Ok(injector) => self.injector = Some(injector),
                    Err(e) => {
                        log::warn!("taskbar clock: could not write the helper: {e}");
                        self.status(Status::Failed);
                        return None;
                    }
                }
            }
            if let Some(host) = self.injector.as_ref().and_then(inject::Injector::host) {
                // Already loaded by an earlier run of Duri that quit before
                // explorer was confirmed healthy: finish that watch.
                if !std::mem::replace(&mut self.pending_checked, true)
                    && self.unconfirmed.is_none()
                    && let Some(explorer) = self.guard().pending_explorer
                {
                    self.unconfirmed = Some((Instant::now(), explorer));
                }
                return Some(host);
            }
            self.pending_checked = true;
            if self.last_attempt.is_some_and(|at| at.elapsed() < RETRY_AFTER) {
                return None;
            }
            self.last_attempt = Some(Instant::now());
            self.shown = None;

            let Some((taskbar, thread, explorer)) = inject::taskbar() else {
                self.status(Status::NoTaskbar);
                return None;
            };
            let mut guard = self.guard();
            let allowed = guard.before_inject(explorer);
            let _ = self.db.set_json(NS, GUARD_KEY, &guard);
            if !allowed {
                self.block();
                return None;
            }
            self.status(Status::Starting);
            match self.injector.as_mut()?.inject(taskbar, thread) {
                Ok(()) => {
                    self.unconfirmed = Some((Instant::now(), explorer));
                    self.injector.as_ref()?.host()
                }
                Err(e) => {
                    log::warn!("taskbar clock: {e}");
                    self.status(Status::Failed);
                    None
                }
            }
        }

        /// Explorer kept crashing after we loaded into it: switch the
        /// setting off, so it stays off until the user asks again.
        fn block(&mut self) {
            self.blocked = true;
            let mut settings = crate::settings::load(&self.db);
            settings.taskbar_clock = false;
            if let Err(e) = crate::settings::save(&self.app, &self.db, &settings) {
                log::warn!("could not save settings: {e}");
            }
            self.status(Status::Blocked);
        }

        fn confirm_if_healthy(&mut self) {
            let Some((since, explorer)) = self.unconfirmed else { return };
            if since.elapsed() < CONFIRM_AFTER {
                return;
            }
            if inject::taskbar().is_some_and(|(_, _, now)| now == explorer) {
                let mut guard = self.guard();
                guard.confirm();
                let _ = self.db.set_json(NS, GUARD_KEY, &guard);
                self.unconfirmed = None;
            }
        }

        /// The cached forecast for the configured place, starting a fetch in
        /// the background when there is none or it has gone stale.
        fn weather(&self, config: &Config) -> Option<weather::Weather> {
            let (latitude, longitude) = config.place?;
            let key = ((latitude * 1e4).round() as i64, (longitude * 1e4).round() as i64, config.fahrenheit);
            let cache = self.state().weather.clone();
            let mut entry = cache.lock().ok()?;
            if entry.key != Some(key) {
                *entry = WeatherCache { key: Some(key), fetching: entry.fetching, ..Default::default() };
            }
            let due = entry.fetched.is_none_or(|(at, ok)| {
                at.elapsed().as_secs() >= if ok { weather::REFRESH_SECS } else { weather::RETRY_SECS }
            });
            if due && !entry.fetching {
                entry.fetching = true;
                let (cache, fahrenheit) = (cache.clone(), config.fahrenheit);
                let spawned = std::thread::Builder::new().name("duri-weather".into()).spawn(move || {
                    let result = crate::http::get(&weather::forecast_url(latitude, longitude, fahrenheit))
                        .and_then(|json| weather::parse_forecast(&json, fahrenheit));
                    let Ok(mut entry) = cache.lock() else { return };
                    entry.fetching = false;
                    // The place may have changed while this was on its way.
                    if entry.key == Some(key) {
                        entry.fetched = Some((Instant::now(), result.is_ok()));
                        match result {
                            Ok(weather) => entry.data = Some(weather),
                            Err(e) => log::warn!("taskbar clock: weather: {e}"),
                        }
                    }
                });
                if spawned.is_err() {
                    entry.fetching = false;
                }
            }
            entry.data.clone()
        }

        /// Samples what the lines use, or everything while Settings shows
        /// the reference.
        fn sample(&mut self, lines: &[Vec<Part>], config: &Config) -> Snapshot {
            let reference_until = self.state().reference_until.lock().ok().and_then(|until| *until);
            let everything = reference_until.is_some_and(|until| Instant::now() < until);
            let wants = |key: Key| everything || template::uses(lines, key);

            let mut snapshot = Snapshot { city: config.city.clone(), turkish: config.turkish, ..Snapshot::default() };
            if wants(Key::Cpu) {
                let now = crate::system::sample::times();
                if let (Some(previous), Some(now)) = (self.cpu_times, now) {
                    snapshot.cpu = crate::system::cpu_percent(previous, now);
                }
                self.cpu_times = now;
            } else {
                self.cpu_times = None;
            }
            if wants(Key::Ram) || wants(Key::RamUsed) {
                snapshot.ram = crate::system::sample::memory().unwrap_or(0);
                snapshot.ram_used_gb = crate::system::sample::memory_used_gb().unwrap_or(0.0);
            }
            if wants(Key::Battery) {
                snapshot.battery = battery();
            }
            let counters = &mut self.counters;
            if let Some(counter) = counters.gpu.counter(wants(Key::Gpu), perf::GPU) {
                let engines = counter.read();
                snapshot.gpu = perf::busiest_engine(engines.iter().map(|(name, value)| (name.as_str(), *value)));
            }
            if let Some(counter) = counters.down.counter(wants(Key::Down), perf::NETWORK_DOWN) {
                snapshot.down = counter.total();
            }
            if let Some(counter) = counters.up.counter(wants(Key::Up), perf::NETWORK_UP) {
                snapshot.up = counter.total();
            }
            if let Some(counter) = counters.disk_read.counter(wants(Key::DiskRead), perf::DISK_READ) {
                snapshot.disk_read = counter.total();
            }
            if let Some(counter) = counters.disk_write.counter(wants(Key::DiskWrite), perf::DISK_WRITE) {
                snapshot.disk_write = counter.total();
            }
            if wants(Key::Song) || wants(Key::Artist) {
                (snapshot.song, snapshot.artist) = crate::apps::music::now_playing(&self.app);
            }
            if everything || template::uses_weather(lines) {
                snapshot.weather = self.weather(config);
            }
            if let Ok(mut shared) = self.state().snapshot.lock() {
                *shared = snapshot.clone();
            }
            snapshot
        }

        fn tick(&mut self, lines: &[Vec<Part>], config: &Config) {
            let Some(host) = self.host() else { return };
            self.confirm_if_healthy();

            let snapshot = self.sample(lines, config);
            let text = template::render_lines(lines, &Live(&snapshot));
            let current = (host.0 as isize, config.align, text);
            if !self.no_clock && self.shown.as_ref() == Some(&current) {
                return;
            }
            // Blank lines only would leave a hole in the taskbar.
            let taken = if current.2.is_empty() {
                inject::revert(host);
                Some(1)
            } else {
                inject::set_text(host, &current.2, config.align, self.generation)
            };
            match taken {
                Some(clocks) => {
                    self.no_clock = clocks == 0;
                    self.shown = Some(current);
                    self.status(if clocks == 0 { Status::NoClock } else { Status::Active });
                }
                // Explorer went away mid-send; the next tick loads again.
                None => self.shown = None,
            }
        }

        /// Everything back to how Windows had it.
        fn stop(&mut self) {
            if self.shown.take().is_some()
                && let Some(host) = self.injector.as_ref().and_then(inject::Injector::host)
            {
                inject::revert(host);
            }
            self.counters = Counters::default();
            self.cpu_times = None;
            self.no_clock = false;
            self.last_attempt = None;
            self.status(if self.blocked { Status::Blocked } else { Status::Off });
        }
    }

    pub fn run(app: AppHandle, db: Arc<Db>, dir: PathBuf, receiver: Receiver<Config>, initial: Config) {
        let mut worker = Worker {
            app,
            db,
            dir,
            injector: None,
            counters: Counters::default(),
            cpu_times: None,
            shown: None,
            no_clock: false,
            last_attempt: None,
            unconfirmed: None,
            pending_checked: false,
            blocked: false,
            generation: 0,
        };
        let mut config = initial;
        loop {
            if !config.enabled {
                worker.stop();
                // Off costs nothing: sleep until the settings change.
                match receiver.recv() {
                    Ok(next) => {
                        if next.enabled {
                            worker.blocked = false;
                        }
                        config = next;
                    }
                    Err(_) => return,
                }
                continue;
            }
            let lines = config.parsed();
            worker.generation = worker.generation.wrapping_add(1);
            loop {
                worker.tick(&lines, &config);
                match receiver.recv_timeout(until_next_second()) {
                    // Some other setting was saved.
                    Ok(next) if next == config => {}
                    Ok(next) => {
                        config = next;
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_blocks_after_explorer_dies_twice() {
        let mut guard = Guard::default();
        assert!(guard.before_inject(100));
        // Same explorer (Duri restarted): no strike.
        assert!(guard.before_inject(100));
        assert_eq!(guard.strikes, 0);
        // Explorer 100 died before it was confirmed.
        assert!(guard.before_inject(200));
        assert_eq!(guard.strikes, 1);
        assert!(!guard.before_inject(300));
        assert_eq!(guard.pending_explorer, None);
    }

    #[test]
    fn guard_forgets_once_explorer_survives() {
        let mut guard = Guard::default();
        assert!(guard.before_inject(100));
        assert!(guard.before_inject(200));
        guard.confirm();
        assert!(guard.before_inject(300));
        assert!(guard.before_inject(400));
        assert_eq!(guard.strikes, 1);
    }

    #[test]
    fn alignment_and_language() {
        assert_eq!(align_code("left"), 0);
        assert_eq!(align_code("center"), 1);
        assert_eq!(align_code("right"), 2);
        assert_eq!(align_code(""), 2);
        assert!(is_turkish("tr"));
        assert!(!is_turkish("en"));
    }
}
