#![feature(test, scoped_tls)]

extern crate tango;

extern crate tempdir;
extern crate test;
extern crate walkdir;

use tango::timestamp::{Timestamp, Timestamped};

use tempdir::TempDir;
use walkdir::{WalkDir};

use std::convert;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{PathBuf};
use std::process::{Command};

const BINARY_FILENAME: &'static str = "tango";
const PRESERVE_TEMP_DIRS: bool = false;
const REPORT_DIR_CONTENTS: bool = false;

const REJECT_IF_TANGO_AFFECTS_STD_PORTS: bool = true;

fn out_path() -> PathBuf {
    let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| {
        panic!("tango tests expect `cargo` to set OUT_DIR; \
                now it does not? Fix me.");
    });
    PathBuf::from(&out_dir)
}

fn infer_target_dir() -> PathBuf {
    let out_path = out_path();

    let mut target_components = out_path.components().rev();
    let mut result = PathBuf::new();
    while let Some(next) = target_components.next() {
        if next.as_os_str() == "build" {
            for comp in target_components.rev() {
                result.push(comp.as_os_str());
            }
            break;
        }
    }

    return result;
}

fn infer_target_binary() -> PathBuf {
    let mut dir = infer_target_dir();
    dir.push(BINARY_FILENAME);
    dir
}

scoped_thread_local!(static CURRENT_DIR_PREFIX: PathBuf);

fn within_temp_dir<F, X>(name: &str, f: F) -> X where F: FnOnce() -> X {
    let out_path = out_path();
    let mut errors = vec![];

    if !out_path.as_path().exists() {
        let mut fail_count = 0;

        while let Err(e) = fs::create_dir_all(&out_path) {
            fail_count += 1;
            if fail_count > 100 {
                panic!("failure to create output directory at {:?} due to {}",
                       &out_path, e);
            } else {
                errors.push((e, &out_path));
            }
        }
    }

    if errors.len() > 0 {
        println!("FYI encountered transient errors {:?} during out_path creation.",
                 errors);
    }


    let temp_dir = TempDir::new_in(&out_path, name)
        .unwrap_or_else(|e| {
            panic!("failure to create temp dir in {:?}: {}", out_path, e);
        });

    let result = CURRENT_DIR_PREFIX.set(&temp_dir.path().to_path_buf(), f);

    if PRESERVE_TEMP_DIRS {
        std::mem::forget(temp_dir);
    } else {
        match temp_dir.close() {
            Ok(()) => {}
            Err(e) => {
                println!("Error cleaning up temp dir {:?}", e);
            }
        }
    }

    result
}

fn indent_at_newline(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        r.push(c);
        if c == '\n' {
            r.push_str("    ");
        }
    }
    r
}

trait UnwrapOrPanic { type X; fn unwrap_or_panic(self, msg: &str) -> Self::X; }
impl<X, Y:Error> UnwrapOrPanic for Result<X, Y> {
    type X = X;
    fn unwrap_or_panic(self, s: &str) -> X {
        self.unwrap_or_else(|e| {
            panic!("{} due to {}", s, indent_at_newline(e.description()));
        })
    }
}

fn setup_src_and_lit_dirs() {
    CURRENT_DIR_PREFIX.with(|p| {
        let mut p_src = p.clone();
        p_src.push(tango::SRC_DIR);
        fs::create_dir(p_src).unwrap_or_panic(&format!("failed to create {}", tango::SRC_DIR));
        if tango::LIT_DIR == tango::SRC_DIR { return; }
        let mut p_lit = p.clone();
        p_lit.push(tango::LIT_DIR);
        fs::create_dir(p_lit).unwrap_or_panic(&format!("failed to create {}", tango::LIT_DIR));
    })
}

enum Target { Root, Src, Lit }

impl Target {
    fn path_buf(&self, filename: &str) -> PathBuf {
        CURRENT_DIR_PREFIX.with(|p| {
            let mut p = p.clone();
            match *self {
                Target::Root => {}
                Target::Src => p.push(tango::SRC_DIR),
                Target::Lit => p.push(tango::LIT_DIR),
            }
            p.push(filename);
            p
        })
    }
}

fn create_file(t: Target, filename: &str, content: &str, timestamp: Timestamp) -> io::Result<()> {
    let p = t.path_buf(filename);
    let p = p.as_path();
    assert!(!p.exists(), "path {:?} should not exist", p);
    let mut f = try!(File::create(p));
    try!(write!(f, "{}", content));
    try!(f.flush());
    drop(f);
    assert!(p.exists(), "path {:?} must now exist", p);
    assert!(timestamp > 0);
    timestamp.set_file_times(p)
}

fn touch_file(t: Target, filename: &str, timestamp: Timestamp) -> Result<(), TangoRunError> {
    let p = t.path_buf(filename);
    let p = p.as_path();
    assert!(p.exists(), "path {:?} should exist", p);
    match () {
        #[cfg(not(unix))]
        () => {}
        #[cfg(unix)]
        () => {
            use std::os::unix::fs::MetadataExt;
            println!("touch path {} t {:?}  pre: {} ", p.display(), timestamp,
                     try!(p.metadata()).mtime());
        }
    }
    assert!(timestamp > 0);
    let ret = timestamp.set_file_times(p).map_err(TangoRunError::IoError);
    let p = t.path_buf(filename);
    let p = p.as_path();
    // let f = try!(File::open(p));
    // try!(f.sync_all());
    match () {
        #[cfg(not(unix))]
        () => {}
        #[cfg(unix)]
        () => {
            use std::os::unix::fs::MetadataExt;
            println!("touch path {} t {:?} post: {} ", p.display(), timestamp,
                     try!(p.metadata()).mtime());
        }
    }
    ret
}

const HELLO_WORLD_RS: &'static str = "
fn main() { println!(\"Hello World\"); }
";

const HELLO_WORLD_MD: &'static str = "
```rust
fn main() { println!(\"Hello World\"); }
```
";

const HELLO_WORLD2_RS: &'static str = "
fn main() { println!(\"Hello World 2\"); }
";

const HELLO_WORLD2_MD: &'static str = "
```rust
fn main() { println!(\"Hello World 2\"); }
```
";

// work-around for lack of stable const fn.
macro_rules! timestamp {
    ($ms:expr) => {
        Timestamp { secs: $ms / 1_000,
                    nsecs: ($ms % 1_000) * 1_000_000 }
    }
}

#[allow(dead_code)] const TIME_A1: Timestamp = timestamp!(1000_000_000);
#[allow(dead_code)] const TIME_A2: Timestamp = timestamp!(1000_100_000);
#[allow(dead_code)] const TIME_A3: Timestamp = timestamp!(1000_200_000);
#[allow(dead_code)] const TIME_B1: Timestamp = timestamp!(2000_000_000);
#[allow(dead_code)] const TIME_B2: Timestamp = timestamp!(2000_100_000);
#[allow(dead_code)] const TIME_B3: Timestamp = timestamp!(2000_200_000);
#[allow(dead_code)] const TIME_C1: Timestamp = timestamp!(3000_000_000);
#[allow(dead_code)] const TIME_C2: Timestamp = timestamp!(3000_100_000);
#[allow(dead_code)] const TIME_C3: Timestamp = timestamp!(3000_200_000);

#[derive(Debug)]
enum TangoRunError {
    IoError(io::Error),
    SawOutput { stdout_len: usize, stderr_len: usize,
                stdout: String, stderr: String, combined: String },
}

impl fmt::Display for TangoRunError {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TangoRunError::IoError(_) =>
                write!(w, "IO error running `tango`"),
            TangoRunError::SawOutput { .. } =>
                write!(w, "`tango` should not produce output"),
        }
    }
}

impl Error for TangoRunError {
    fn description(&self) -> &str {
        match *self {
            TangoRunError::IoError(ref e) => e.description(),
            TangoRunError::SawOutput {
                stdout_len, stderr_len, stdout: ref o, stderr: ref e, combined: ref c
            } => {
                match (stdout_len > 0, stderr_len > 0) {
                    (true, true) => c,
                    (true, false) => o,
                    (false, true) => e,
                    (false, false) => panic!("did not SawOutput"),
                }
            }
        }
    }
}

impl convert::From<io::Error> for TangoRunError {
    fn from(e: io::Error) -> Self {
        TangoRunError::IoError(e)
    }
}

fn run_tango() -> Result<(), TangoRunError> {
    CURRENT_DIR_PREFIX.with(|p| -> Result<(), TangoRunError> {
        let result = infer_target_binary();
        // println!("result {:?}", result);
        let output = match Command::new(result)
            .current_dir(p)
            .output() {
                Ok(o) => o,
                Err(e) => return Err(TangoRunError::IoError(e)),
            };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if REJECT_IF_TANGO_AFFECTS_STD_PORTS &&
            stdout.len() > 0 || stderr.len() > 0
        {
            return Err(TangoRunError::SawOutput {
                stdout_len: stdout.len(),
                stderr_len: stderr.len(),
                stdout: format!("output on stdout: `{}`", stdout),
                stderr: format!("output on stderr: `{}`", stderr),
                combined: format!("output on stderr: `{err}`, stdout: `{out}`",
                                  err=stderr, out=stdout),
            });
        } else {
            for line in stdout.lines() {
                println!("stdout: {}", line);
            }
            for line in stderr.lines() {
                println!("stderr: {}", line);
            }
        }
        Ok(())
    })
}

#[cfg(unix)]
fn report_dir_contents(prefix: &str) {
    #![allow(deprecated)]
    use std::os::unix::fs::MetadataExt;
    if !REPORT_DIR_CONTENTS { return; }
    CURRENT_DIR_PREFIX.with(|p| {
        for (i, ent) in WalkDir::new(p).into_iter()
            .enumerate()
        {
            match ent {
                Ok(ent) => {
                    // println!("entry[{}]: {:?}", i, ent.file_name());
                    println!("{} entry[{}]: {:?}",
                             prefix, i, ent.path());
                    match ent.metadata() {
                        Err(e) => {
                            println!("{} failed to extract metadata for {:?} due to {:?}",
                                     prefix, ent.file_name(), e.description());
                        }
                        Ok(m) => {
                            // println!("{} entry[{}] metadata accessed: {:?}",
                            //          prefix, i, m.accessed());
                            println!("{} entry[{}] metadata modified: {:?}",
                                     prefix, i, m.mtime());
                        }
                    }
                }
                Err(e) => {
                    println!("{} entry[{}]: error due to {:?}",
                             prefix, i, e.description());
                }
            }
        }
    })
}

struct Test<SETUP, PRE, RUN, POST> {
    name: &'static str,
    setup: SETUP,
    pre: PRE,
    run: RUN,
    post: POST,
}

fn framework<S, PR, RUN, PO>(test: Test<S, PR, RUN, PO>) -> Result<(), TangoRunError> where
    S: FnOnce() -> Result<(), TangoRunError>,
   PR: FnOnce() -> Result<(), TangoRunError>,
  RUN: FnOnce() -> Result<(), TangoRunError>,
   PO: FnOnce() -> Result<(), TangoRunError>,
{
    within_temp_dir(test.name, move || -> Result<(), TangoRunError> {
        let Test { name: _, setup, pre, run, post } = test;
        println!("Setup test");
        setup_src_and_lit_dirs();
        try!(setup());

        report_dir_contents("before");
        println!("Check pre-conditions");
        try!(pre());

        println!("Run the action");
        try!(run());

        report_dir_contents("after");
        println!("Check post-conditions");
        try!(post());

        Ok(())
    })
}

//@ ## Test Matrix
//@
//@ We use a kernel of five files to model filesystem timestamp-based
//@ actions: `{ STAMP, MD1, MD2, RS1, RS2 }`, where the filename for
//@ `MDi` corresponds to the filename for `RSi`.
//@
//@ First, we consider every subset of the kernel. Then, since
//@ generally all that matters is the ordering (and not the values) of
//@ the modification timestamps, we then enumerate the permutations its
//@ set-partitions. These orderings correspond to the orderings of the
//@ modification timestamps.
//@
//@ So, for example, for the subset `{ MD1, MD2, RS1 }`, the
//@ set-partitions are:
//@
//@ ```
//@ { MD1 }{ MD2 }{ RS1 };
//@ { MD1 MD2 }{ RS1 }; { MD1 RS1 }{ MD2 }; { MD2 RS1 }{ MD1 };
//@ { MD1 MD2 RS1 }
//@ ```
//@
//@ and then extending those to the full set of permutations adds in
//@ the additional elements:
//@
//@ ```
//@ { MD1 }{ RS1 }{ MD2 }; { MD2 }{ MD1 }{ RS1 }; { MD2 }{ RS1 }{ MD1 };
//@ { RS1 }{ MD2 }{ MD1 }; { RS1 }{ MD1 }{ MD2 };
//@ { RS1 }{ MD1 MD2 }; { MD2 }{ MD1 RS1 }; { MD1 }{ MD2 RS1 };
//@ ```
//@
//@ Interpretation: A test case basis such as `{ MD1 RS1 }{ MD2 }`
//@ represents the case where `MD1` and `RS1` have the same timestamp,
//@ and `MD2` has a distinct, newer timestamp.

#[test]
fn unstamped_and_src_without_lit() {
    framework(Test {
        name: "unstamped_and_src_without_lit",
        setup: || {
            try!(create_file(Target::Src, "foo.rs", HELLO_WORLD_RS, TIME_B1));
            Ok(())
        },
        // Check pre-conditions
        pre: || {
            assert!(!Target::Lit.path_buf("foo.md").exists());
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Root.path_buf(tango::STAMP).exists());
            assert!(Target::Lit.path_buf("foo.md").exists());
            // TODO: check contents
            // TODO: check timestamps
            Ok(())
        },
    }).unwrap_or_panic("test error")
}

#[test]
fn unstamped_and_lit_without_src() {
    framework(Test {
        name: "unstamped_and_lit_without_src",
        setup: || {
            try!(create_file(Target::Lit, "foo.md", HELLO_WORLD_MD, TIME_B1));
            Ok(())
        },
        // Check pre-conditions
        pre: || {
            assert!(!Target::Src.path_buf("foo.rs").exists());
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Root.path_buf(tango::STAMP).exists());
            assert!(Target::Src.path_buf("foo.rs").exists());
            // TODO: check contents
            // TODO: check timestamps
            Ok(())
        },
    }).unwrap_or_panic("test error")
}

#[test]
fn stamp_and_src_without_lit() {
    framework(Test {
        name: "stamp_and_src_without_lit",
        setup: || {
            try!(create_file(Target::Root, tango::STAMP, "", TIME_A1));
            try!(create_file(Target::Src, "foo.rs", HELLO_WORLD_RS, TIME_B1));
            Ok(())
        },
        // Check pre-conditions
        pre: || {
            assert!(!Target::Lit.path_buf("foo.md").exists());
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Root.path_buf(tango::STAMP).exists());
            assert!(Target::Lit.path_buf("foo.md").exists());
            // TODO: check contents
            // TODO: check timestamps
            Ok(())
        },
    }).unwrap_or_panic("test error")
}

#[test]
fn stamp_and_lit_without_src() {
    framework(Test {
        name: "stamp_and_lit_without_src",
        setup: || {
            try!(create_file(Target::Root, tango::STAMP, "", TIME_A1));
            try!(create_file(Target::Lit, "foo.md", HELLO_WORLD_MD, TIME_B1));
            Ok(())
        },
        pre: || {
            assert!(!Target::Src.path_buf("foo.rs").exists());
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Root.path_buf(tango::STAMP).exists());
            assert!(Target::Src.path_buf("foo.rs").exists());
            // TODO: check contents
            // TODO: check timestamps
            Ok(())
        },
    }).unwrap_or_panic("test error")
}

#[test]
fn stamped_then_touch_lit() {
    framework(Test {
        name: "stamped_then_touch_lit",
        setup: || {
            try!(create_file(Target::Lit, "foo.md", HELLO_WORLD_MD, TIME_B1));
            assert!(!Target::Src.path_buf("foo.rs").exists());
            try!(run_tango());
            touch_file(Target::Lit, "foo.md", TIME_B2)
        },
        pre: || {
            assert!(Target::Src.path_buf("foo.rs").exists());
            assert!(Target::Lit.path_buf("foo.md").exists());
            let rs_t = try!(Target::Src.path_buf("foo.rs").metadata()).timestamp();
            let md_t = try!(Target::Lit.path_buf("foo.md").metadata()).timestamp();
            assert!(TIME_B1 == rs_t, "rs_t: {:?} TIME_B1: {:?}", rs_t, TIME_B1);
            assert!(TIME_B2 == md_t, "md_t: {:?} TIME_B2: {:?}", md_t, TIME_B2);
            assert!(TIME_B2 > TIME_B1);
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Lit.path_buf("foo.md").exists());
            assert!(Target::Src.path_buf("foo.rs").exists());
            let rs_t = try!(Target::Src.path_buf("foo.rs").metadata()).timestamp();
            let md_t = try!(Target::Lit.path_buf("foo.md").metadata()).timestamp();
            assert!(TIME_B2 == rs_t, "rs_t: {:?} TIME_B2: {:?}", rs_t, TIME_B2);
            assert!(TIME_B2 == md_t, "md_t: {:?} TIME_B2: {:?}", md_t, TIME_B2);
            // TODO: check contents
            Ok(())
        }
    }).unwrap_or_panic("test error")
}

#[test]
fn stamped_then_touch_src() {
    framework(Test {
        name: "stamped_then_touch_src",
        setup: || {
            try!(create_file(Target::Src, "foo.rs", HELLO_WORLD_RS, TIME_B1));
            assert!(!Target::Lit.path_buf("foo.md").exists());
            try!(run_tango());
            touch_file(Target::Src, "foo.rs", TIME_B2)
        },
        pre: || {
            assert!(Target::Src.path_buf("foo.rs").exists());
            assert!(Target::Lit.path_buf("foo.md").exists());
            println!("try rs_t");
            let rs_t = try!(Target::Src.path_buf("foo.rs").metadata()).timestamp();
            println!("try md_t");
            let md_t = try!(Target::Lit.path_buf("foo.md").metadata()).timestamp();
            assert!(TIME_B1 == md_t, "md_t: {:?} TIME_B1: {:?}", md_t, TIME_B1);
            assert!(TIME_B2 == rs_t, "rs_t: {:?} TIME_B2: {:?}", rs_t, TIME_B2);
            assert!(TIME_B2 > TIME_B1);
            Ok(())
        },
        run: run_tango,
        post: || {
            assert!(Target::Lit.path_buf("foo.md").exists());
            assert!(Target::Src.path_buf("foo.rs").exists());
            let rs_t = try!(Target::Src.path_buf("foo.rs").metadata()).timestamp();
            let md_t = try!(Target::Lit.path_buf("foo.md").metadata()).timestamp();
            assert!(TIME_B2 == rs_t, "rs_t: {:?} TIME_B2: {:?}", rs_t, TIME_B2);
            assert!(TIME_B2 == md_t, "md_t: {:?} TIME_B2: {:?}", md_t, TIME_B2);
            // TODO: check contents
            Ok(())
        }
    }).unwrap_or_panic("test error")
}

#[test]
fn stamped_then_update_src() {
    framework(Test {
        name: "stamped_then_update_src",
        setup: || {
            let rs_path = &Target::Src.path_buf("foo.rs");
            let md_path = &Target::Lit.path_buf("foo.md");
            assert!(!md_path.exists());
            try!(create_file(Target::Lit, "foo.md", HELLO_WORLD_MD, TIME_B1));
            assert!(!rs_path.exists());
            try!(run_tango());
            assert!(rs_path.exists());
            let mut f = try!(File::create(rs_path));
            try!(write!(f, "{}", HELLO_WORLD2_RS));
            try!(f.flush());
            drop(f);
            touch_file(Target::Src, "foo.rs", TIME_B2)
        },
        pre: || {
            let rs_path = &Target::Src.path_buf("foo.rs");
            let md_path = &Target::Lit.path_buf("foo.md");
            assert!(rs_path.exists());
            assert!(md_path.exists());
            let rs_t = try!(rs_path.metadata()).timestamp();
            let md_t = try!(md_path.metadata()).timestamp();
            assert!(TIME_B1 == md_t, "md_t: {:?} TIME_B1: {:?}", md_t, TIME_B1);
            assert!(TIME_B2 == rs_t, "rs_t: {:?} TIME_B2: {:?}", rs_t, TIME_B2);
            assert!(TIME_B2 > TIME_B1);
            Ok(())
        },
        run: run_tango,
        post: || {
            let rs_path = &Target::Src.path_buf("foo.rs");
            let md_path = &Target::Lit.path_buf("foo.md");
            assert!(md_path.exists());
            assert!(rs_path.exists());
            let rs_t = try!(rs_path.metadata()).timestamp();
            let md_t = try!(md_path.metadata()).timestamp();
            assert!(TIME_B2 == rs_t, "rs_t: {:?} TIME_B2: {:?}", rs_t, TIME_B2);
            assert!(TIME_B2 == md_t, "md_t: {:?} TIME_B2: {:?}", md_t, TIME_B2);
            let mut f = try!(File::open(md_path));
            let mut s = String::new();
            try!(f.read_to_string(&mut s));
            assert!(s == HELLO_WORLD2_MD);
            Ok(())
        }
    }).unwrap_or_panic("test error")
}
