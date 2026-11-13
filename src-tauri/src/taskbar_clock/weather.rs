//! Weather from Open-Meteo (https://open-meteo.com): free, no key, no
//! account. The only thing sent is the latitude and longitude of the city
//! the user picked in Settings; nothing is sent while no weather token is in
//! use.

use crate::http::percent_encode;
use serde::{Deserialize, Serialize};

/// How long a forecast is good for, and how soon to retry after a failure.
pub const REFRESH_SECS: u64 = 15 * 60;
pub const RETRY_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    /// `YYYY-MM-DD` in the city's own time zone.
    pub date: String,
    pub code: u16,
    pub high: f64,
    pub low: f64,
    /// Highest chance of rain or snow that day, 0 – 100.
    pub rain: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weather {
    pub temp: f64,
    pub feels: f64,
    pub humidity: u8,
    pub wind: f64,
    pub code: u16,
    pub is_day: bool,
    pub fahrenheit: bool,
    /// Today first, then the coming days.
    pub days: Vec<Day>,
}

/// A city found by name.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    /// Province or state, then country: tells apart cities with one name.
    pub region: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// WMO weather code → emoji, English, Turkish.
fn describe(code: u16, is_day: bool) -> (&'static str, &'static str, &'static str) {
    match code {
        0 => (if is_day { "☀️" } else { "🌙" }, "Clear", "Açık"),
        1 => (if is_day { "🌤️" } else { "🌙" }, "Mostly clear", "Az bulutlu"),
        2 => ("⛅", "Partly cloudy", "Parçalı bulutlu"),
        3 => ("☁️", "Cloudy", "Bulutlu"),
        45 | 48 => ("🌫️", "Fog", "Sisli"),
        51 | 53 | 55 => ("🌦️", "Drizzle", "Çisenti"),
        56 | 57 | 66 | 67 => ("🌧️", "Freezing rain", "Dondurucu yağmur"),
        61 | 63 => ("🌧️", "Rain", "Yağmurlu"),
        65 => ("🌧️", "Heavy rain", "Şiddetli yağmur"),
        71 | 73 | 77 => ("🌨️", "Snow", "Karlı"),
        75 => ("❄️", "Heavy snow", "Yoğun kar"),
        80..=82 => ("🌦️", "Showers", "Sağanak"),
        85 | 86 => ("🌨️", "Snow showers", "Kar sağanağı"),
        95 => ("⛈️", "Thunderstorm", "Gök gürültülü fırtına"),
        96 | 99 => ("⛈️", "Hailstorm", "Dolu fırtınası"),
        _ => ("🌡️", "Unknown", "Bilinmiyor"),
    }
}

pub fn icon(code: u16, is_day: bool) -> &'static str {
    describe(code, is_day).0
}

pub fn condition(code: u16, is_day: bool, turkish: bool) -> &'static str {
    let (_, english, tr) = describe(code, is_day);
    if turkish { tr } else { english }
}

pub fn degrees(value: f64) -> String {
    // No "-0°".
    let rounded = value.round();
    format!("{}°", if rounded == 0.0 { 0.0 } else { rounded })
}

impl Weather {
    pub fn wind_text(&self) -> String {
        format!("{} {}", self.wind.round(), if self.fahrenheit { "mph" } else { "km/h" })
    }

    /// The next `days` days as "Fri ☀️ 21°  Sat 🌧️ 17°". `day_name` turns a
    /// `YYYY-MM-DD` date into a short weekday name.
    pub fn forecast(&self, days: u8, day_name: &dyn Fn(&str) -> String) -> String {
        self.days
            .iter()
            .skip(1)
            .take(days as usize)
            .map(|day| format!("{} {} {}", day_name(&day.date), icon(day.code, true), degrees(day.high)))
            .collect::<Vec<_>>()
            .join("  ")
    }
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: Current,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: f64,
    wind_speed_10m: f64,
    weather_code: u16,
    is_day: u8,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    weather_code: Vec<Option<u16>>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    precipitation_probability_max: Vec<Option<f64>>,
}

pub fn forecast_url(latitude: f64, longitude: f64, fahrenheit: bool) -> String {
    let units = if fahrenheit { "&temperature_unit=fahrenheit&wind_speed_unit=mph" } else { "" };
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude:.4}&longitude={longitude:.4}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max\
         &timezone=auto&forecast_days=8{units}"
    )
}

pub fn parse_forecast(json: &str, fahrenheit: bool) -> Result<Weather, String> {
    let response: ForecastResponse = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let daily = response.daily;
    let days = daily
        .time
        .iter()
        .enumerate()
        .map(|(i, date)| Day {
            date: date.clone(),
            code: daily.weather_code.get(i).copied().flatten().unwrap_or(u16::MAX),
            high: daily.temperature_2m_max.get(i).copied().flatten().unwrap_or(f64::NAN),
            low: daily.temperature_2m_min.get(i).copied().flatten().unwrap_or(f64::NAN),
            rain: daily.precipitation_probability_max.get(i).copied().flatten().unwrap_or(0.0).clamp(0.0, 100.0) as u8,
        })
        .collect();
    let current = response.current;
    Ok(Weather {
        temp: current.temperature_2m,
        feels: current.apparent_temperature,
        humidity: current.relative_humidity_2m.clamp(0.0, 100.0) as u8,
        wind: current.wind_speed_10m,
        code: current.weather_code,
        is_day: current.is_day != 0,
        fahrenheit,
        days,
    })
}

pub fn search_url(query: &str, turkish: bool) -> String {
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=6&format=json&language={}",
        percent_encode(query.trim()),
        if turkish { "tr" } else { "en" }
    )
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    country: Option<String>,
}

pub fn parse_search(json: &str) -> Result<Vec<Place>, String> {
    let response: SearchResponse = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(response
        .results
        .into_iter()
        .map(|result| {
            let region = [result.admin1, result.country]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty() && *part != result.name)
                .collect::<Vec<_>>()
                .join(", ");
            Place { name: result.name, region, latitude: result.latitude, longitude: result.longitude }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::get as http_get;

    const FORECAST: &str = r#"{
        "latitude": 41.0, "longitude": 29.0, "timezone": "Europe/Istanbul",
        "current": {"time": "2026-11-12T14:30", "interval": 900, "temperature_2m": 17.6,
            "apparent_temperature": 16.2, "relative_humidity_2m": 62, "wind_speed_10m": 11.9,
            "weather_code": 2, "is_day": 1},
        "daily": {"time": ["2026-11-12", "2026-11-13", "2026-11-14", "2026-11-15"],
            "weather_code": [2, 61, 0, null],
            "temperature_2m_max": [18.4, 15.1, -0.2, null],
            "temperature_2m_min": [11.0, 9.6, -4.5, null],
            "precipitation_probability_max": [10, 85, 0, null]}
    }"#;

    #[test]
    fn reads_a_forecast() {
        let weather = parse_forecast(FORECAST, false).unwrap();
        assert_eq!(degrees(weather.temp), "18°");
        assert_eq!(degrees(weather.feels), "16°");
        assert_eq!(weather.humidity, 62);
        assert_eq!(weather.wind_text(), "12 km/h");
        assert_eq!(condition(weather.code, weather.is_day, false), "Partly cloudy");
        assert_eq!(condition(weather.code, weather.is_day, true), "Parçalı bulutlu");
        assert_eq!(weather.days.len(), 4);
        assert_eq!(weather.days[1].rain, 85);
        assert_eq!(degrees(weather.days[2].high), "0°");
    }

    #[test]
    fn forecast_line_starts_tomorrow() {
        let weather = parse_forecast(FORECAST, false).unwrap();
        let name = |date: &str| date[8..].to_string();
        assert_eq!(weather.forecast(2, &name), "13 🌧️ 15°  14 ☀️ 0°");
        assert_eq!(weather.forecast(1, &name), "13 🌧️ 15°");
    }

    #[test]
    fn reads_search_results() {
        let json = r#"{"results": [
            {"id": 1, "name": "İzmir", "latitude": 38.41, "longitude": 27.13, "country": "Türkiye", "admin1": "İzmir"},
            {"id": 2, "name": "Springfield", "latitude": 39.8, "longitude": -89.6, "country": "United States", "admin1": "Illinois"}
        ]}"#;
        let places = parse_search(json).unwrap();
        assert_eq!(places[0].region, "Türkiye");
        assert_eq!(places[1].region, "Illinois, United States");
        assert_eq!(parse_search(r#"{"generationtime_ms": 0.3}"#).unwrap(), vec![]);
    }

    /// The real service: `cargo test -- --ignored live_`.
    #[cfg(windows)]
    #[test]
    #[ignore = "talks to open-meteo.com"]
    fn live_search_and_forecast() {
        let places = parse_search(&http_get(&search_url("İstanbul", true)).unwrap()).unwrap();
        let place = places.first().expect("no city found");
        let weather = parse_forecast(&http_get(&forecast_url(place.latitude, place.longitude, false)).unwrap(), false).unwrap();
        assert!(weather.days.len() >= 8, "{} days", weather.days.len());
        println!(
            "{} ({}): {} {} {}, {}",
            place.name,
            place.region,
            icon(weather.code, weather.is_day),
            degrees(weather.temp),
            condition(weather.code, weather.is_day, true),
            weather.forecast(7, &|date| date[5..].to_string())
        );
    }

    #[test]
    fn urls() {
        assert!(search_url(" İzmir ", true).contains("name=%C4%B0zmir&"));
        assert!(search_url("x", true).ends_with("language=tr"));
        let url = forecast_url(41.00827, 28.97836, true);
        assert!(url.contains("latitude=41.0083&longitude=28.9784"));
        assert!(url.ends_with("&temperature_unit=fahrenheit&wind_speed_unit=mph"));
    }
}
