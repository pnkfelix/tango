// #[macro_use]
// extern crate log;
// extern crate env_logger;

extern crate filetime;
extern crate url;
extern crate walkdir;

use filetime::set_file_times;
use walkdir::{WalkDir};

use std::convert;
use std::error::Error as ErrorTrait;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::ops;
use std::path::{Path, PathBuf};
use std::cell::RefCell;

use self::timestamp::{Timestamp, Timestamped};

pub mod timestamp;

pub const STAMP: &'static str = "tango.stamp";
//pub const SRC_DIR: &'static str = "src";
// pnkfelix wanted the `LIT_DIR` to be `lit/`, but `cargo build`
// currently assumes that *all* build sources live in `src/`. So it
// is easier for now to just have the two directories be the same.
//pub const LIT_DIR: &'static str = "src/lit";

thread_local! {
    pub static SRC_DIR: RefCell<String> = RefCell::new("src".to_string());
    pub static LIT_DIR: RefCell<String> = RefCell::new("src".to_string());
}

fn set_lit_dir(directory: String) {
    LIT_DIR.with(|lit_dir| {
        *lit_dir.borrow_mut() = directory
    });
}

fn set_src_dir(directory: String) {
    SRC_DIR.with(|src_dir| {
        *src_dir.borrow_mut() = directory
    });
}

/// Returns the current directory for storing the literate .md files
pub fn get_lit_dir() -> String {
    LIT_DIR.with(|lit_dir| lit_dir.borrow().clone())
}

/// Returns the current directory for storing the "source" .rs files
pub fn get_src_dir() -> String {
    SRC_DIR.with(|src_dir| src_dir.borrow().clone())
}

pub struct Config {
    src_dir: String,
    lit_dir: String,
    rerun_if: bool,
}

impl Config {
    pub fn new() -> Config {
        Config {
            src_dir: String::from("src"),
            lit_dir: String::from("src"),
            rerun_if: false,
        }
    }
    pub fn set_src_dir(&mut self, new_src_dir: String) -> &mut Config {
        self.src_dir = new_src_dir;
        self
    }
    pub fn set_lit_dir(&mut self, new_lit_dir: String) -> &mut Config {
        self.lit_dir = new_lit_dir;
        self
    }
    pub fn emit_rerun_if(&mut self) -> &mut Config {
        self.rerun_if = true;
        self
    }

}


#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    CheckInputError { error: check::Error },
    MtimeError(PathBuf),
    ConcurrentUpdate { path_buf: PathBuf, old_time: mtime, new_time: mtime },
    Warnings(Vec<Warning>),
}

#[derive(Debug)]
pub enum Warning {
    EncodedUrlMismatch { actual: String, expect: String }
}

impl fmt::Display for Warning {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Warning::EncodedUrlMismatch { ref actual, ref expect } => {
                write!(w, "mismatch between encoded url, expect: {} actual: {}",
                       expect, actual)
            }
        }
    }
}

impl From<md2rs::Exception> for Error {
    fn from(e: md2rs::Exception) -> Self {
        match e {
            md2rs::Exception::IoError(e) => Error::IoError(e),
            md2rs::Exception::Warnings(w) => Error::Warnings(w),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::IoError(_) =>
                write!(w, "IO error running `tango`"),
            Error::CheckInputError { .. } =>
                write!(w, "input check errors running `tango`"),
            Error::MtimeError(ref p) =>
                write!(w, "modification time error from `tango` checking {}",
                       p.to_string_lossy()),
            Error::ConcurrentUpdate { ref path_buf, .. } =>
                write!(w, "concurrent update during `tango` to source file {}",
                       path_buf.to_string_lossy()),
            Error::Warnings(ref warnings) => {
                for warn in warnings {
                    (write!(w, "WARNING: {}", warn))?;
                }
                Ok(())
            }
        }
    }
}

impl ErrorTrait for Error {
    fn description(&self) -> &str {
        match *self {
            Error::IoError(ref e) => e.description(),
            Error::CheckInputError { ref error } => {
                error.description()
            }
            Error::MtimeError(_) => "Modification time check error",
            Error::ConcurrentUpdate { .. } => "concurrent update",
            Error::Warnings(_) => "warnings",
        }
    }
    fn cause(&self) -> Option<&ErrorTrait> {
        match *self {
            Error::IoError(ref e) => Some(e),
            Error::CheckInputError { ref error, .. } => {
                Some(error)
            }
            Error::Warnings(_) |
            Error::MtimeError(_) |
            Error::ConcurrentUpdate { .. } => None,
        }
    }
}

impl convert::From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IoError(e)
    }
}

impl convert::From<walkdir::Error> for Error {
    fn from(e: walkdir::Error) -> Self {
        Error::IoError(From::from(e))
    }
}

pub type Result<X> = std::result::Result<X, Error>;

#[allow(non_camel_case_types)]
pub type mtime = Timestamp;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MtimeResult {
    NonExistant,
    Modified(mtime),
}

trait Mtime { fn modified(&self) -> Result<MtimeResult>; }
impl Mtime for File {
    fn modified(&self) -> Result<MtimeResult> {
        // #![allow(deprecated)]
        // if let Some(p) = self.path() {
        //     if !p.exists() {
        //         return Err(Error::MtimeError(p.to_path_buf()));
        //     }
        // }
        let m = (self.metadata())?;
        Ok(MtimeResult::Modified(m.timestamp()))
    }
}
impl Mtime for fs::DirEntry {
    fn modified(&self) -> Result<MtimeResult> {
        let m = (self.metadata())?;
        Ok(MtimeResult::Modified(m.timestamp()))
    }
}
impl Mtime for RsPath {
    fn modified(&self) -> Result<MtimeResult> {
        if self.0.exists() {
            let f = (File::open(&self.0))?;
            f.modified()
        } else {
            Ok(MtimeResult::NonExistant)
        }
    }
}
impl Mtime for MdPath {
    fn modified(&self) -> Result<MtimeResult> {
        if self.0.exists() {
            let f = (File::open(&self.0))?;
            f.modified()
        } else {
            Ok(MtimeResult::NonExistant)
        }
    }
}

pub fn process_root_with_config(config: Config) -> Result<()> {
    //let _root = (std::env::current_dir())?;
    //println!("Tango is running from: {:?}", root);
    //std::env::set_current_dir(_root).unwrap();
    set_lit_dir(config.lit_dir);
    set_src_dir(config.src_dir);
    let emit_rerun_if = config.rerun_if;

    let stamp_path = Path::new(STAMP);
    if stamp_path.exists() {
        process_with_stamp((File::open(stamp_path))?, emit_rerun_if)
    } else {
        process_without_stamp(emit_rerun_if)
    }
}


pub fn process_root() -> Result<()> {
    //let _root = (std::env::current_dir())?;
    // println!("Tango is running from: {:?}", _root);

    let emit_rerun_if = false;
    let stamp_path = Path::new(STAMP);
    if stamp_path.exists() {
        process_with_stamp((File::open(stamp_path))?, emit_rerun_if)
    } else {
        process_without_stamp(emit_rerun_if)
    }
}

// Both of the functions below have the same basic outline:
//
// 1. gather_inputs(): Build up a list of potential transforms based
//    on existing files.
//
// 2. generate_content(): Apply each transform in turn, *iff* the
//    source is newer than target.
//
// 3. check_input_timestamps(): Ensure no input was concurrently
//    modified while tango ran.
//
// 4. adjust_stamp_timestamp(): Update the `tango.stamp` file to the
//    youngest timestamp we saw, creating the file if necessary.
//
// The reason there are two functions is that in one case we have a
// pre-existing `tango.stamp` that we want to compare against during
// `generate_content()` (to guard against diverging {source, target}
// paths; *at most* one of {source, target} is meant to be updated in
// between tango runs.
//
// (It probably wouldn't be hard to unify the two functions into a
//  single method on the `Context`, though.)

fn process_with_stamp(stamp: File, emit_rerun_if: bool) -> Result<()> {
    println!("\n\nemit rerun if: {:?}\n\n", emit_rerun_if);
    if let Ok(MtimeResult::Modified(ts)) = stamp.modified() {
        println!("Rerunning tango; last recorded run was stamped: {}",
                 ts.date_fulltime_badly());
    } else {
        panic!("why are we trying to process_with_stamp when given: {:?}", stamp);
    }
    let mut c = (Context::new(Some(stamp)))?;
    c.emit_rerun_if = emit_rerun_if;
    (c.gather_inputs())?;
    (c.generate_content())?;
    (c.check_input_timestamps())?;
    (c.adjust_stamp_timestamp())?;
    // (c.report_dir(Path::new(".")))?;
    Ok(())
}

fn process_without_stamp(emit_rerun_if: bool) -> Result<()> {
    println!("Running tango; no previously recorded run");
    println!("\n\nemit rerun if: {:?}\n\n", emit_rerun_if);
    let mut c = (Context::new(None))?;
    c.emit_rerun_if = emit_rerun_if;
    (c.gather_inputs())?;
    (c.generate_content())?;
    (c.check_input_timestamps())?;
    (c.create_stamp())?;
    (c.adjust_stamp_timestamp())?;
    // (c.report_dir(Path::new(".")))?;
    Ok(())
}

#[derive(Debug)]
struct RsPath(PathBuf);
#[derive(Debug)]
struct MdPath(PathBuf);


struct Context {
    orig_stamp: Option<(File, mtime)>,
    src_inputs: Vec<Transform<RsPath, MdPath>>,
    lit_inputs: Vec<Transform<MdPath, RsPath>>,
    newest_stamp: Option<mtime>,
    emit_rerun_if: bool,
}

trait Extensions {
    fn extension(&self) -> Option<&str>;
    fn rs_extension(&self) -> bool {
        self.extension() == Some("rs")
    }
    fn md_extension(&self) -> bool {
        self.extension() == Some("md")
    }
}

impl Extensions for Path {
    fn extension(&self) -> Option<&str> {
        Path::extension(self).and_then(|s|s.to_str())
    }
}

impl ops::Deref for RsPath {
    type Target = Path; fn deref(&self) -> &Path { &self.0 }
}

impl ops::Deref for MdPath {
    type Target = Path; fn deref(&self) -> &Path { &self.0 }
}

fn check_path(typename: &str, p: &Path, ext: &str, root: &str) {
    println!("\n in check_path, the root is: {r:?} , path is: {p:?}, ext is {e:?}", r=root, p=p, e=ext);
    if Extensions::extension(p) != Some(ext) { panic!("{t} requires `.{ext}` extension; path: {p:?}", t=typename, ext=ext, p=p); }
    if !p.starts_with(root) { panic!("{t} must be rooted at `{root}/`; path: {p:?}", t=typename, root=root, p=p); }
}

impl RsPath {
    fn new(p: PathBuf) -> RsPath {
        check_path("RsPath", &p, "rs", &get_src_dir());
        RsPath(p)
    }
    fn to_md(&self) -> MdPath {
        let mut p = PathBuf::new();
        p.push(get_lit_dir());
        for c in self.0.components().skip(1) {
            let c: &OsStr = c.as_ref();
            p.push(c.to_str().expect("how else can I replace root?"));
        }
        p.set_extension("md");
        MdPath::new(p)
    }
}

impl MdPath {
    fn new(p: PathBuf) -> MdPath {
        check_path("MdPath", &p, "md", &get_lit_dir());
        MdPath(p)
    }
    fn to_rs(&self) -> RsPath {
        let mut p = PathBuf::new();
        p.push(get_src_dir());
        for c in self.0.components().skip(1) {
            let c: &OsStr = c.as_ref();
            p.push(c.to_str().expect("how else can I replace root?"));
        }
        p.set_extension("rs");
        RsPath::new(p)
    }
}

trait Transforms: Sized + Mtime + fmt::Debug {
    type Target: Mtime + fmt::Debug;

    // Computes path to desired target based on self's (source) path.
    fn target(&self) -> Self::Target;

    // Constructs a transform for generating the target from self
    // (which is a path to the source), gathering the current
    // timestamps on both the source and the target.
    fn transform(self) -> Result<Transform<Self, Self::Target>> {
        let source_time = match self.modified() {
            Ok(MtimeResult::Modified(t)) => t,
            Ok(MtimeResult::NonExistant) => panic!("impossible for {:?} to be NonExistant", self),
            Err(e) => {
                println!("failure to extract mtime on source {:?}", self);
                return Err(e);
            }
        };

        let target = self.target();
        let target_time = match target.modified() {
            Ok(t) => t,
            Err(e) => {
                println!("failure to extract mtime on target {:?}", target);
                return Err(e);
            }
        };
        Ok(Transform { source_time: source_time,
                       target_time: target_time,
                       original: self,
                       generate: target,
        })
    }
}

impl Transforms for RsPath {
    type Target = MdPath;
    fn target(&self) -> MdPath { self.to_md() }
}

impl Transforms for MdPath {
    type Target = RsPath;
    fn target(&self) -> RsPath { self.to_rs() }
}

#[derive(Debug)]
pub struct Transform<X, Y> {
    source_time: mtime,
    target_time: MtimeResult,
    original: X,
    generate: Y,
}

pub mod check {
    use std::error::Error as ErrorTrait;
    use std::fmt;
    use std::ops;
    use std::path::{Path, PathBuf};
    use std::result;
    use super::Transform;
    pub type PathTransform = Transform<PathBuf, PathBuf>;
    #[derive(Debug)]
    pub enum ErrorKind {
        TargetYoungerThanOriginal { tgt: String, src: String },
        NoTangoStampExists { tgt: String, src: String },
        TangoStampOlderThanTarget { tgt: String },
    }
    #[derive(Debug)]
    pub struct Error(ErrorKind, PathTransform);

    impl fmt::Display for Error {
        fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
            match self.0 {
                ErrorKind::TargetYoungerThanOriginal { ref tgt, ref src } => {
                    write!(w, "target `{}` is younger than source `{}`; \
                               therefore we assume target has modifications that need to be preserved.",
                           tgt, src)
                }
                ErrorKind::NoTangoStampExists { ref src, ref tgt } => {
                    write!(w, "both source `{}` and target `{}` exist but no `tango.stamp` is present",
                           src, tgt)
                }
                ErrorKind::TangoStampOlderThanTarget { ref tgt } => {
                    write!(w, "`tango.stamp` is older than target `{}`; \
                               therefore we assume source and target have diverged since last tango run.",
                           tgt)
                }
            }
        }
    }

    impl ErrorTrait for Error {
        fn description(&self) -> &str {
            match self.0 {
                ErrorKind::TargetYoungerThanOriginal { .. }=> {
                    "target is younger than source; \
                     therefore we assume target has modifications that need to be preserved."
                }
                ErrorKind::NoTangoStampExists { .. } => {
                    "both source and target exist but no `tango.stamp` is present"
                }
                ErrorKind::TangoStampOlderThanTarget { .. } => {
                    "`tango.stamp` is older than target; \
                     therefore we assume source and target have diverged since last tango run."
                }
            }
        }
    }

    pub type Result<X> = result::Result<X, Error>;

    impl<X,Y> Transform<X, Y>
        where X: ops::Deref<Target=Path>, Y: ops::Deref<Target=Path>
    {
        pub fn error(&self, kind: ErrorKind) -> Error {
            let t = Transform { original: self.original.to_path_buf(),
                                generate: self.generate.to_path_buf(),
                                source_time: self.source_time,
                                target_time: self.target_time,
            };
            Error(kind, t)
        }
    }
}

enum TransformNeed { Needed, Unneeded, }

impl Context {
    fn new(opt_stamp: Option<File>) -> Result<Context> {
        let stamp_modified = match opt_stamp {
            None => None,
            Some(stamp) => {
                let mtime = (stamp.modified())?;
                let mtime = match mtime {
                    MtimeResult::NonExistant => panic!("impossible"),
                    MtimeResult::Modified(t) => t,
                };
                Some((stamp, mtime))
            }
        };
        let c = Context {
            orig_stamp: stamp_modified,
            src_inputs: Vec::new(),
            lit_inputs: Vec::new(),
            newest_stamp: None,
            emit_rerun_if: true,
        };
        Ok(c)
    }

    fn check_transform<X, Y>(&self, t: &Transform<X, Y>) -> check::Result<TransformNeed>
        where X: ops::Deref<Target=Path> + Mtime,
              Y: ops::Deref<Target=Path> + Mtime,
    {

        use self::check::ErrorKind::*;


        let t_mod = match t.target_time {
            MtimeResult::Modified(t) => t,
            MtimeResult::NonExistant => {
                assert!(!t.generate.exists());
                return Ok(TransformNeed::Needed);
            }
        };
        // let src = t.original.display().to_string();
        // let tgt = t.generate.display().to_string();
        let s_mod = t.source_time;

        let same_age_at_low_precision = s_mod.to_ms() == t_mod.to_ms();

        if t_mod > s_mod {
            // Target is newer than source: therefore we do not want to
            // overwrite the target via this transform.
            return Ok(TransformNeed::Unneeded);
        }

        // Now know:  t_mod <= s_mod

        if same_age_at_low_precision {
            //        00000000011111111112222222222333333333344444444445555555555666666666677777777778
            //        12345678901234567890123456789012345678901234567890123456789012345678901234567890
            println!("Warning: source and target have timestamps that differ only at nanosecond level\n    \
                          precision. Tango currently treats such timestamps as matching, and therefore\n    \
                          will not rebuild the target file.\n\
                          \n    \
                          source: {SRC:?} timestamp: {SRC_TS} \n    \
                          target: {TGT:?} timestamp: {TGT_TS}\n",
                     SRC=t.original.display(), SRC_TS=s_mod.date_fulltime_badly(),
                     TGT=t.generate.display(), TGT_TS=t_mod.date_fulltime_badly());
            return Ok(TransformNeed::Unneeded);
        }

        // Now know: t_mod is older than source even after truncating
        // to millisecond precision.

        match self.orig_stamp {
            None => return Err(t.error(NoTangoStampExists {
                src: t.original.display().to_string(),
                tgt: t.generate.display().to_string(),
            })),
            Some((_, stamp_time)) => {
                let older_at_high_precision = stamp_time < t_mod;
                let older_at_low_precision = stamp_time.to_ms() < t_mod.to_ms();
                if older_at_low_precision {
                    // The target file was updated more recently than
                    // the tango.stamp file, even after truncation to
                    // millisecond precision.
                    //
                    // Therefore, we assume that user has updated both
                    // the source and the target independently since
                    // the last tango run.  This is a scenario that
                    // tango cannot currently recover from, so we
                    // issue an error and tell the user to fix the
                    // problem.
                    return Err(t.error(TangoStampOlderThanTarget {
                        tgt: t.generate.display().to_string(),
                    }));
                }
                if older_at_high_precision && !older_at_low_precision {
                    //        00000000011111111112222222222333333333344444444445555555555666666666677777777778
                    //        12345678901234567890123456789012345678901234567890123456789012345678901234567890
                    println!("Warning: `tango.stamp` and target `{}` have timestamps that differ only at \n\
                                  nanosecond level precision. Tango currently treats such timestamps as,\n\
                                  matching and will rebuild the target file rather than error",
                             t.generate.display());
                }

                // got here: tango.stamp is not older than the target
                // file.  So we fall through to the base case.
            }
        }

        // Invariant:
        // Target `t` exists, but,
        // s_mod >= t_mod (and t_mod <= stamp_time if stamp exists).
        //
        // Thus it is safe to overwrite `t` based on source content.
        Ok(TransformNeed::Needed)
    }

    #[cfg(not_now)]
    fn report_dir(&self, p: &Path) -> Result<()> {
        let src_dir = get_src_dir();
        let lit_dir = get_lit_dir();
        let src_path = Path::new(&src_dir);
        let lit_path = Path::new(&lit_dir);

        for (i, ent) in (WalkDir::new(p))?.enumerate() {
            let ent = (ent)?;
            let modified = (ent.modified())?;
            println!("entry[{}]: {:?} {:?}", i, ent.path(), modified);
        }
        Ok(())
    }

    fn update_newest_time(&mut self, new_time: mtime) {
        if let Some(ref mut stamp) = self.newest_stamp {
            if new_time > *stamp {
                *stamp = new_time;
            }
        } else {
            self.newest_stamp = Some(new_time);
        }
    }

    fn push_src(&mut self, t: Transform<RsPath, MdPath>) {
        self.update_newest_time(t.source_time);
        self.src_inputs.push(t);
    }
    fn push_lit(&mut self, t: Transform<MdPath, RsPath>) {
        self.update_newest_time(t.source_time);
        self.lit_inputs.push(t);
    }

    fn gather_inputs(&mut self) -> Result<()> {
        // println!("gather_inputs");
        let src_dir = get_src_dir();
        let lit_dir = get_lit_dir();
        let src_path = Path::new(&src_dir);
        let lit_path = Path::new(&lit_dir);

        fn keep_file_name(p: &Path) -> std::result::Result<(), &'static str> {
            match p.file_name().and_then(|x|x.to_str()) {
                None =>
                    Err("file name is not valid unicode"),
                Some(s) if s.starts_with('.') =>
                    Err("file name has leading period"),
                Some(..) =>
                    Ok(()),
            }
        }

        fn warn_if_nonexistant<M:Mtime+fmt::Debug>(m: &M) -> Result<()> {
            match m.modified() {
                Err(e) => Err(e),
                Ok(MtimeResult::Modified(..)) => Ok(()),
                Ok(MtimeResult::NonExistant) => {
                    // This can arise; namely some tools are
                    // generating symlinks in `src` of the form
                    //
                    // `src/.#lib.md -> fklock@fklock-Oenone.local.96195`
                    //
                    // where the target is non-existant (presumably as
                    // a way to locally mark a file as being open by
                    // the tool?), and then this script interprets it
                    // as being open.
                    println!("warning: non-existant source: {:?}", m);
                    Ok(())
                }
            }

        }

        // This loop gathers all of the .rs files that currently
        // exist, and schedules transforms that would turn them into
        // corresponding target .md files.

        // println!("gather-rs");
        for ent in WalkDir::new(src_path).into_iter() {
            let ent = (ent)?;
            let p = ent.path();
            if let Err(why) = keep_file_name(p) {
                println!("skipping {}; {}", p.display(), why);
                continue;
            }
            if !p.rs_extension() {
                // println!("gather-rs skip {} due to non .rs", p.display());
                continue;
            }
            let rs = RsPath::new(p.to_path_buf());
            (warn_if_nonexistant(&rs))?;

            if self.emit_rerun_if {
                println!("cargo:rerun-if-changed={}", &rs.display());
            }

            let t = (rs.transform())?;
            match self.check_transform(&t) {
                Ok(TransformNeed::Needed) => self.push_src(t),
                Ok(TransformNeed::Unneeded) => {}
                Err(e) => {
                    println!("gather_inputs err: {}", e.description());
                    return Err(Error::CheckInputError {
                        error: e,
                    })
                }
            }
        }

        // This loop gathers all of the .md files that currently
        // exist, and schedules transforms that would turn them into
        // corresponding target .rs files.

        //println!("gather-md, lit_path is: {:?}", lit_path);
        for ent in WalkDir::new(lit_path).into_iter() {
            //println!("ent is {:?}", ent);
            let ent = (ent)?;
            let p = ent.path();
            if let Err(why) = keep_file_name(p) {
                println!("skipping {}; {}", p.display(), why);
                continue;
            }
            if !p.md_extension() {
                // println!("gather-md skip {} due to non .md", p.display());
                continue;
            }
            let md = MdPath::new(p.to_path_buf());
            (warn_if_nonexistant(&md))?;

            if self.emit_rerun_if {
                println!("cargo:rerun-if-changed={}", &md.display());
            }

            let t = (md.transform())?;
            match self.check_transform(&t) {
                Ok(TransformNeed::Needed) => {
                    // println!("gather-md add {:?}", t);;
                    self.push_lit(t)
                }
                Ok(TransformNeed::Unneeded) => {
                    // println!("gather-md discard unneeded {:?}", t);;
                }
                Err(e) => {
                    println!("gather_inputs err: {}", e.description());
                    return Err(Error::CheckInputError {
                        error: e,
                    })
                }
            }
        }

        // At this point we've scheduled all the transforms we want to
        // run; they will be applied unconditionally, even if both
        // source and target exist. (The intent is that a target
        // younger than source would have been filtered during the
        // .check_transform calls above.)

        Ok(())
    }
    fn generate_content(&mut self) -> Result<()> {
        for &Transform { ref original, ref generate, source_time, .. } in &self.src_inputs {
            let source = (File::open(&original.0))?;
            let target = (File::create(&generate.0))?;
            assert!(source_time > 0);
            println!("generating lit {:?}", &generate.0);
            (rs2md(source, target))?;
            let timestamp = source_time.to_filetime();
            println!("backdating lit {:?} to {}", &generate.0, source_time.date_fulltime_badly());
            (set_file_times(&generate.0, timestamp, timestamp))?;
        }
        for &mut Transform { ref original, ref generate, ref mut source_time, .. } in &mut self.lit_inputs {
            let source = (File::open(&original.0))?;
            let target = (File::create(&generate.0))?;
            assert!(*source_time > 0);
            println!("generating src {:?}", &generate.0);
            (md2rs(source, target))?;
            println!("backdating src {:?} to {}", &generate.0, source_time.date_fulltime_badly());
            (set_file_times(&generate.0,
                                source_time.to_filetime(),
                                source_time.to_filetime()))?;
            let source = (File::open(&original.0))?;
            let target = (File::open(&generate.0))?;
            match (source.modified(), target.modified()) {
                (Ok(MtimeResult::Modified(src_time)),
                 Ok(MtimeResult::Modified(tgt_time))) => {
                    // At this point, we would *like* to assert this:
                    #[cfg(not_possible_right_now)] assert_eq!(src_time, tgt_time);
                    // but it does not work, due to this bug:
                    // https://github.com/alexcrichton/filetime/issues/9

                    assert_eq!(src_time.to_ms(), tgt_time.to_ms());
                }
                (Ok(MtimeResult::NonExistant), _) => panic!("how could source not exist"),
                (_, Ok(MtimeResult::NonExistant)) => panic!("how could target not exist"),
                (Err(_), Err(_)) => panic!("errored looking up both source and target times"),
                (Err(_), _) => panic!("errored looking up source time"),
                (_, Err(_)) => panic!("errored looking up target time"),
            }
        }
        Ok(())
    }
    fn check_input_timestamps(&mut self) -> Result<()> {
        for &Transform { ref original, source_time, .. } in &self.src_inputs {
            if let MtimeResult::Modified(new_time) = (original.modified())? {
                if new_time != source_time {
                    return Err(Error::ConcurrentUpdate {
                        path_buf: original.to_path_buf(),
                        old_time: source_time,
                        new_time: new_time,
                    })
                }
            }
        }
        for &Transform { ref original, source_time, .. } in &self.lit_inputs {
            if let MtimeResult::Modified(new_time) = (original.modified())? {
                if new_time != source_time {
                    return Err(Error::ConcurrentUpdate {
                        path_buf: original.to_path_buf(),
                        old_time: source_time,
                        new_time: new_time,
                    })
                }
            }
        }
        Ok(())
    }
    fn create_stamp(&mut self) -> Result<()> {
        let _f = (File::create(STAMP))?;
        Ok(())
    }
    fn adjust_stamp_timestamp(&mut self) -> Result<()> {
        if let Some(stamp) = self.newest_stamp {
            assert!(stamp > 0);
            println!("re-stamping tango.stamp to {}", stamp.date_fulltime_badly());

            match set_file_times(STAMP, stamp.to_filetime(), stamp.to_filetime()) {
                Ok(()) => Ok(()),
                Err(e) => Err(Error::IoError(e)),
            }
        } else {
            Ok(())
        }
    }
}

fn rs2md<R:Read, W:Write>(source: R, target: W) -> Result<()> {
    let mut converter = rs2md::Converter::new();
    converter.convert(source, target).map_err(Error::IoError)
}

fn md2rs<R:Read, W:Write>(source: R, target: W) -> Result<()> {
    let converter = md2rs::Converter::new();
    converter.convert(source, target).map_err(From::from)
}

mod md2rs;

mod rs2md;

fn encode_to_url(code: &str) -> String {
    use url::percent_encoding as enc;
    // let new_code: String = enc::utf8_percent_encode(code.trim(), enc::QUERY_ENCODE_SET);
    let new_code: String = enc::utf8_percent_encode(code.trim(), enc::USERINFO_ENCODE_SET).collect();
    format!("https://play.rust-lang.org/?code={}&version=nightly", new_code)
}

#[cfg(test)]
mod testing;
