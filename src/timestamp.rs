use filetime::{self, FileTime};

use std::cmp::{self, PartialEq, PartialOrd};
use std::fs;
use std::io;
use std::path::Path;

pub trait Timestamped {
    fn timestamp(&self) -> Timestamp;
}

#[derive(Copy, Clone, PartialEq, Eq, Ord, Debug)]
pub struct Timestamp { pub secs: u64, pub nsecs: u64 }

#[allow(non_snake_case)]
pub fn Timestamp(ms: u64) -> Timestamp  {
    Timestamp::new(ms / 1_000, (ms % 1_000) * 1_000_000)
}

impl Timestamp {
    pub fn new(secs: u64, ns: u64) -> Timestamp {
        Timestamp { secs: secs, nsecs: ns }
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
}

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
        let ft = FileTime::from_last_modification_time( self );
        let s = ft.seconds_relative_to_1970();
        let ns = ft.nanoseconds();
        // println!("metadata mtime: {} ns: {}", s, ns);
        Timestamp::new(s as u64, ns as u64)
    }
}
