//! Windows performance counters (PDH): GPU usage, network and disk
//! throughput. These are the sources Task Manager reads.

use std::collections::HashMap;

pub const GPU: &str = "\\GPU Engine(*)\\Utilization Percentage";
pub const NETWORK_DOWN: &str = "\\Network Interface(*)\\Bytes Received/sec";
pub const NETWORK_UP: &str = "\\Network Interface(*)\\Bytes Sent/sec";
pub const DISK_READ: &str = "\\PhysicalDisk(_Total)\\Disk Read Bytes/sec";
pub const DISK_WRITE: &str = "\\PhysicalDisk(_Total)\\Disk Write Bytes/sec";

/// `pid_1234_luid_0x0_0xD3A5_phys_0_eng_3_engtype_VideoDecode` → the part
/// from `luid_` on, which names one engine of one adapter.
fn engine_key(instance: &str) -> &str {
    instance.find("luid_").map_or(instance, |at| &instance[at..])
}

/// GPU usage from the GPU Engine instances, each one engine of one adapter
/// for one process: the busiest engine, summed over processes.
pub fn busiest_engine<'a>(samples: impl IntoIterator<Item = (&'a str, f64)>) -> u8 {
    let mut engines: HashMap<&str, f64> = HashMap::new();
    for (instance, value) in samples {
        if value.is_finite() && value > 0.0 {
            *engines.entry(engine_key(instance)).or_default() += value;
        }
    }
    engines.values().fold(0.0_f64, |max, &value| max.max(value)).round().clamp(0.0, 100.0) as u8
}

#[cfg(windows)]
pub use pdh::Counter;

#[cfg(windows)]
mod pdh {
    use windows::core::HSTRING;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    };

    const PDH_MORE_DATA: u32 = 0x8000_07D2;

    /// One open counter. Values are averages since the previous read, so the
    /// first read reports nothing.
    pub struct Counter {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }

    impl Counter {
        /// `path` is the English counter path; it works on every display
        /// language.
        pub fn open(path: &str) -> Option<Self> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(None, 0, &mut query) != 0 {
                    return None;
                }
                let mut counter = PDH_HCOUNTER::default();
                if PdhAddEnglishCounterW(query, &HSTRING::from(path), 0, &mut counter) != 0 {
                    PdhCloseQuery(query);
                    return None;
                }
                PdhCollectQueryData(query);
                Some(Self { query, counter })
            }
        }

        /// Every instance of the counter with its value.
        pub fn read(&mut self) -> Vec<(String, f64)> {
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return Vec::new();
                }
                let (mut size, mut count) = (0u32, 0u32);
                if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
                    != PDH_MORE_DATA
                {
                    return Vec::new();
                }
                // The buffer holds the items followed by their name strings;
                // u64s keep it aligned for the items.
                let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
                let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                    return Vec::new();
                }
                std::slice::from_raw_parts(items, count as usize)
                    .iter()
                    .filter(|item| item.FmtValue.CStatus == 0 && !item.szName.is_null())
                    .map(|item| (item.szName.to_string().unwrap_or_default(), item.FmtValue.Anonymous.doubleValue))
                    .collect()
            }
        }

        /// All instances added up (every network adapter, for example).
        pub fn total(&mut self) -> f64 {
            self.read().iter().map(|(_, value)| value).filter(|value| value.is_finite()).sum::<f64>().max(0.0)
        }
    }

    impl Drop for Counter {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::busiest_engine;

    #[test]
    fn sums_processes_per_engine_and_takes_the_busiest() {
        let samples = [
            ("pid_100_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 30.4),
            ("pid_200_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 12.0),
            ("pid_100_luid_0x0_0xA_phys_0_eng_3_engtype_VideoDecode", 25.0),
            ("pid_300_luid_0x0_0xB_phys_0_eng_0_engtype_3D", 5.0),
        ];
        assert_eq!(busiest_engine(samples), 42);
    }

    #[test]
    fn empty_and_overshoot() {
        assert_eq!(busiest_engine([]), 0);
        assert_eq!(busiest_engine([("pid_1_luid_a_eng_0", 180.0), ("pid_2_luid_a_eng_0", f64::NAN)]), 100);
    }
}
