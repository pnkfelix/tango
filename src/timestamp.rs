use filetime::{self, FileTime};

use std::cmp::{self, PartialEq, PartialOrd};
use std::fs;
use std::io;
use std::path::Path;

pub trait Timestamped {
    fn timestamp(&self) -> Timestamp;
}

#[derive(Copy, Clone, PartialEq, Eq, Ord, Debug)]
pub struct Timestamp {
    pub secs: u64,
    pub nsecs: u64,
}

#[allow(non_snake_case)]
pub fn Timestamp(ms: u64) -> Timestamp {
    Timestamp::new(ms / 1_000, (ms % 1_000) * 1_000_000)
}

impl Timestamp {
    pub fn new(secs: u64, ns: u64) -> Timestamp {
        Timestamp {
            secs: secs,
            nsecs: ns,
        }
    }
    pub fn to_filetime(&self) -> FileTime {
        assert!(self.nsecs < ::std::u32::MAX as u64);
        FileTime::from_seconds_since_1970(self.secs, self.nsecs as u32)
    }
    pub fn to_ms(&self) -> u64 {
        self.secs * 1000 + self.nsecs / 1_000_000
    }
    pub fn set_file_times<P: AsRef<Path>>(&self, p: P) -> io::Result<()> {
        let t = self.to_filetime();
        filetime::set_file_times(p, t, t)
    }
    pub fn date_fulltime_badly(&self) -> String {
        // TODO: throw away this function if/when something like Joda
        // time is available as a Rust crate.

        // (Seconds since January 1, 1970).
        let mut remain = self.to_filetime().seconds_relative_to_1970();

        let mut year = None;
        for y in 1970.. {
            let secs_per_year = secs_per_year(y);
            if remain > secs_per_year {
                remain -= secs_per_year;
            } else {
                year = Some(y);
                break;
            }
        }
        let year = year.unwrap();

        let mut month = None;
        for i in 0..12 {
            let secs_per_month = if is_leap_year(year) {
                SECS_PER_DAY * DAYS_PER_MONTH_IN_LEAP[i]
            } else {
                SECS_PER_DAY * DAYS_PER_MONTH_IN_COMMON[i]
            };
            if remain > secs_per_month {
                remain -= secs_per_month;
            } else {
                month = Some(i + 1); // We count months starting from 1 ...
                break;
            }
        }
        let month = month.unwrap();

        let day = remain / SECS_PER_DAY + 1; // ... and we count days starting from 1
        let remain = remain % SECS_PER_DAY;

        let hour = remain / SECS_PER_HOUR; // ... but we count hours from zero (military time)
        let remain = remain % SECS_PER_HOUR;

        let min = remain / SECS_PER_MIN; // ... and likewise count minutes from zero
        let remain = remain % SECS_PER_MIN;

        let sec = remain; // ... and likewise count seconds from zero
        let nsec = self.nsecs; // ... et cetera.

        format!(
            "{YEAR:04}-{MONTH:02}-{DAY:02} {HOUR:02}:{MIN:02}:{SEC:02}.{NSEC} (GMT)",
            YEAR = year,
            MONTH = month,
            DAY = day,
            HOUR = hour,
            MIN = min,
            SEC = sec,
            NSEC = nsec
        )
    }
}

fn is_leap_year(gregorian_year: u64) -> bool {
    let year = gregorian_year;
    if !(year % 4 == 0) {
        false
    } else if !(year % 100 == 0) {
        true
    } else if !(year % 400 == 0) {
        false
    } else {
        true
    }
}

fn secs_per_year(gregorian_year: u64) -> u64 {
    if is_leap_year(gregorian_year) {
        SECS_PER_LEAP_YEAR
    } else {
        SECS_PER_COMMON_YEAR
    }
}

const SECS_PER_LEAP_YEAR: u64 = 366 * SECS_PER_DAY;
const SECS_PER_COMMON_YEAR: u64 = 365 * SECS_PER_DAY;
const DAYS_PER_MONTH_IN_LEAP: [u64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const DAYS_PER_MONTH_IN_COMMON: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;
const SECS_PER_HOUR: u64 = 60 * SECS_PER_MIN;
const SECS_PER_MIN: u64 = 60;

impl PartialEq<u64> for Timestamp {
    fn eq(&self, other: &u64) -> bool {
        self.to_ms().eq(other)
    }
}

impl PartialEq<i64> for Timestamp {
    fn eq(&self, other: &i64) -> bool {
        if *other < 0 {
            false
        } else {
            let other = *other as u64;
            self.to_ms().eq(&other)
        }
    }
}

impl PartialOrd<u64> for Timestamp {
    fn partial_cmp(&self, other: &u64) -> Option<cmp::Ordering> {
        self.to_ms().partial_cmp(other)
    }
}

impl PartialOrd<Timestamp> for Timestamp {
    fn partial_cmp(&self, other: &Timestamp) -> Option<cmp::Ordering> {
        match self.secs.partial_cmp(&other.secs) {
            Some(cmp::Ordering::Equal) => self.nsecs.partial_cmp(&other.nsecs),
            otherwise => otherwise,
        }
    }
}

impl Timestamped for fs::Metadata {
    fn timestamp(&self) -> Timestamp {
        let ft = FileTime::from_last_modification_time(self);
        let s = ft.seconds_relative_to_1970();
        let ns = ft.nanoseconds();
        // println!("metadata mtime: {} ns: {}", s, ns);
        Timestamp::new(s as u64, ns as u64)
    }
}
