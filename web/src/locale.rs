use jiff::Timestamp;
use jiff::tz::Offset;
use ruffle_core::backend::locale::LocaleBackend;

/// Reads the current date/time and timezone from the browser.
///
/// This avoids bundling the IANA Time Zone Database into the WASM binary,
/// which jiff would otherwise need to resolve the system timezone.
#[derive(Default)]
pub struct WebLocaleBackend {}

impl WebLocaleBackend {
    pub fn new() -> Self {
        Self {}
    }
}

impl LocaleBackend for WebLocaleBackend {
    fn get_current_date_time(&self) -> Timestamp {
        Timestamp::now()
    }

    fn get_timezone(&self) -> Offset {
        // `getTimezoneOffset` returns UTC minus local time, in minutes.
        let minutes = js_sys::Date::new_0().get_timezone_offset();
        Offset::from_seconds(-(minutes * 60.0) as i32).unwrap_or(Offset::UTC)
    }
}
