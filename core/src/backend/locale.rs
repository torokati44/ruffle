use jiff::Timestamp;
use jiff::civil::date;
use jiff::tz::{Offset, TimeZone};

/// Provides locale-specific environment data.
pub trait LocaleBackend {
    fn get_current_date_time(&self) -> Timestamp;

    fn get_timezone(&self) -> Offset;
}

/// Reads the current date/time and timezone from the host system.
pub struct DefaultLocaleBackend {}

impl DefaultLocaleBackend {
    pub fn new() -> Self {
        Self {}
    }
}

impl LocaleBackend for DefaultLocaleBackend {
    fn get_current_date_time(&self) -> Timestamp {
        Timestamp::now()
    }

    fn get_timezone(&self) -> Offset {
        // Falls back to UTC if the system time zone can't be determined.
        TimeZone::system().to_offset(Timestamp::now())
    }
}

impl Default for DefaultLocaleBackend {
    fn default() -> Self {
        DefaultLocaleBackend::new()
    }
}

/// A `LocaleBackend` with a fixed date/time and timezone, for deterministic
/// execution.
pub struct DeterministicLocaleBackend {
    timestamp: Timestamp,
    timezone: Offset,
}

impl DeterministicLocaleBackend {
    pub fn new(timestamp: Timestamp, timezone: Offset) -> Self {
        Self {
            timestamp,
            timezone,
        }
    }
}

impl LocaleBackend for DeterministicLocaleBackend {
    fn get_current_date_time(&self) -> Timestamp {
        self.timestamp
    }

    fn get_timezone(&self) -> Offset {
        self.timezone
    }
}

impl Default for DeterministicLocaleBackend {
    fn default() -> Self {
        // Emulates being in Nepal with a local time of 2001-02-03 at 04:05:06.
        // Nepal has a timezone offset of +5:45, and has never used DST.
        // This makes it an ideal candidate for fixed tests.
        let timezone = Offset::from_seconds(20700).expect("Valid mock timezone");
        let timestamp = timezone
            .to_timestamp(date(2001, 2, 3).at(4, 5, 6, 0))
            .expect("Valid mock time");
        Self::new(timestamp, timezone)
    }
}
