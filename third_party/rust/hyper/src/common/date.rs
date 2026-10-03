// SPDX-License-Identifier: MIT
#[cfg(all(feature = "http1", feature = "std"))]
use alloc::vec::Vec;
use core::fmt::{self, Write};
#[cfg(feature = "std")]
use std::cell::RefCell;
#[cfg(feature = "std")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "http2")]
use http::header::HeaderValue;

// "Sun, 06 Nov 1994 08:49:37 GMT".len()
pub(crate) const DATE_VALUE_LENGTH: usize = 29;

#[cfg(all(feature = "http1", feature = "std"))]
pub(crate) fn extend(dst: &mut Vec<u8>) {
    CACHED.with(|cache| {
        dst.extend_from_slice(cache.borrow().buffer());
    });
}

#[cfg(all(feature = "http1", feature = "std"))]
pub(crate) fn update() {
    CACHED.with(|cache| {
        cache.borrow_mut().check();
    });
}

#[cfg(feature = "http2")]
pub(crate) fn update_and_header_value() -> HeaderValue {
    CACHED.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.check();
        cache.header_value.clone()
    })
}

/// The date of one second, written as an IMF fixdate.
///
/// With `std` the thread local cache reads the system clock, and without it a
/// connection keeps a cache of its own that its timer's wall clock updates.
pub(crate) struct CachedDate {
    bytes: [u8; DATE_VALUE_LENGTH],
    pos: usize,
    #[cfg(feature = "http2")]
    header_value: HeaderValue,
    secs: u64,
}

#[cfg(feature = "std")]
thread_local!(static CACHED: RefCell<CachedDate> = RefCell::new(CachedDate::new()));

impl CachedDate {
    #[cfg(feature = "std")]
    fn new() -> Self {
        CachedDate::at(system_seconds())
    }

    /// Returns the date of `secs` seconds since the Unix epoch.
    pub(crate) fn at(secs: u64) -> Self {
        let mut cache = CachedDate {
            bytes: [0; DATE_VALUE_LENGTH],
            pos: 0,
            #[cfg(feature = "http2")]
            header_value: HeaderValue::from_static(""),
            secs,
        };
        cache.render(secs);
        cache
    }

    pub(crate) fn buffer(&self) -> &[u8] {
        &self.bytes[..]
    }

    #[cfg(feature = "std")]
    fn check(&mut self) {
        self.check_at(system_seconds());
    }

    /// Writes the date again when `secs` names another second than the cache.
    pub(crate) fn check_at(&mut self, secs: u64) {
        if secs != self.secs {
            self.secs = secs;
            self.render(secs);
        }
    }

    fn render(&mut self, secs: u64) {
        self.pos = 0;
        let _ = write_imf_fixdate(self, secs);
        debug_assert_eq!(self.pos, DATE_VALUE_LENGTH);
        self.render_http2();
    }

    #[cfg(feature = "http2")]
    fn render_http2(&mut self) {
        self.header_value = HeaderValue::from_bytes(self.buffer())
            .expect("Date format should be valid HeaderValue");
    }

    #[cfg(not(feature = "http2"))]
    fn render_http2(&mut self) {}
}

/// Returns the seconds since the Unix epoch of the system clock.
#[cfg(feature = "std")]
fn system_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// Writes `secs` seconds since the Unix epoch as an IMF fixdate, the form of
/// RFC 9110 section 5.6.7, "Sun, 06 Nov 1994 08:49:37 GMT".
///
/// The civil date comes from `civil_from_days` of Howard Hinnant's date
/// algorithms, <https://howardhinnant.github.io/date_algorithms.html>, over
/// unsigned days, since no date before the Unix epoch is written. A time past
/// the year 9999 is written as its last second, the latest date of four digits.
fn write_imf_fixdate<W: Write>(dst: &mut W, secs: u64) -> fmt::Result {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let secs = secs.min(253_402_300_799);
    let days = secs / 86_400;
    let time = secs % 86_400;
    // The Unix epoch, day 0, was a Thursday.
    let weekday = (days + 4) % 7;

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);

    write!(
        dst,
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[weekday as usize],
        day,
        MONTHS[month as usize - 1],
        year,
        time / 3_600,
        time / 60 % 60,
        time % 60,
    )
}

impl fmt::Write for CachedDate {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let len = s.len();
        self.bytes[self.pos..self.pos + len].copy_from_slice(s.as_bytes());
        self.pos += len;
        Ok(())
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[cfg(feature = "nightly")]
    use test::Bencher;

    #[test]
    fn test_date_len() {
        assert_eq!(DATE_VALUE_LENGTH, "Sun, 06 Nov 1994 08:49:37 GMT".len());
    }

    #[test]
    fn test_date_render() {
        let render = |secs| {
            let date = CachedDate::at(secs);
            String::from_utf8(date.buffer().to_vec()).unwrap()
        };
        assert_eq!(render(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(render(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(render(951_782_400), "Tue, 29 Feb 2000 00:00:00 GMT");
        assert_eq!(render(253_402_300_799), "Fri, 31 Dec 9999 23:59:59 GMT");
        assert_eq!(render(u64::MAX), "Fri, 31 Dec 9999 23:59:59 GMT");
    }

    #[cfg(feature = "nightly")]
    #[bench]
    fn bench_date_check(b: &mut Bencher) {
        let mut date = CachedDate::new();
        // cache the first update
        date.check();

        b.iter(|| {
            date.check();
        });
    }

    #[cfg(feature = "nightly")]
    #[bench]
    fn bench_date_render(b: &mut Bencher) {
        let mut date = CachedDate::new();
        let now = system_seconds();
        date.render(now);
        b.bytes = date.buffer().len() as u64;

        b.iter(|| {
            date.render(now);
            test::black_box(&date);
        });
    }
}
