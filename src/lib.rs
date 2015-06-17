#![feature(dir_entry_ext, fs_time, fs_walk, path_ext, file_path)]

// #[macro_use]
// extern crate log;
// extern crate env_logger;

use std::convert;
use std::env;
use std::error::Error as ErrorTrait;
use std::fmt;
use std::fs::{self, File, PathExt};
use std::io::{self, Read, Write};
use std::ops;
use std::path::{Path, PathBuf};

pub const STAMP: &'static str = "tango.stamp";
pub const SRC_DIR: &'static str = "src";

// pnkfelix wanted the `LIT_DIR` to be `lit/`, but `cargo build`
// currently assumes that *all* build sources live in `src/`. So it
// is easier for now to just have the two directories be the same.
pub const LIT_DIR: &'static str = "src";

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    CheckInputError { error: check::Error },
    MtimeError(PathBuf),
    ConcurrentUpdate { path_buf: PathBuf, old_time: u64, new_time: u64 },
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
        }
    }
    fn cause(&self) -> Option<&ErrorTrait> {
        match *self {
            Error::IoError(ref e) => Some(e),
            Error::CheckInputError { ref error, .. } => {
                Some(error)
            }
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

pub type Result<X> = std::result::Result<X, Error>;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MtimeResult {
    NonExistant,
    Modified(u64),
}

trait Mtime { fn modified(&self) -> Result<MtimeResult>; }
impl Mtime for File {
    fn modified(&self) -> Result<MtimeResult> {
        #![allow(deprecated)]
        if let Some(p) = self.path() {
            if !p.exists() {
                return Err(Error::MtimeError(p.to_path_buf()));
            }
        }
        let m = try!(self.metadata());
        Ok(MtimeResult::Modified(m.modified()))
    }
}
impl Mtime for fs::DirEntry {
    fn modified(&self) -> Result<MtimeResult> {
        #![allow(deprecated)]
        let m = try!(self.metadata());
        Ok(MtimeResult::Modified(m.modified()))
    }
}
impl Mtime for RsPath {
    fn modified(&self) -> Result<MtimeResult> {
        if self.0.exists() {
            let f = try!(File::open(&self.0));
            f.modified()
        } else {
            Ok(MtimeResult::NonExistant)
        }
    }
}
impl Mtime for MdPath {
    fn modified(&self) -> Result<MtimeResult> {
        if self.0.exists() {
            let f = try!(File::open(&self.0));
            f.modified()
        } else {
            Ok(MtimeResult::NonExistant)
        }
    }
}
pub fn process_root() -> Result<()> {
    let _root = try!(env::current_dir());
    // println!("Tango is running from: {:?}", _root);
    let stamp_path = Path::new(STAMP);
    if stamp_path.exists() {
        process_with_stamp(try!(File::open(stamp_path)))
    } else {
        process_without_stamp()
    }
}

fn process_with_stamp(stamp: File) -> Result<()> {
    let mut c = try!(Context::new(Some(stamp)));
    try!(c.gather_inputs());
    try!(c.generate_content());
    try!(c.check_input_timestamps());
    try!(c.adjust_stamp_timestamp());
    // try!(c.report_dir(Path::new(".")));
    Ok(())
}

fn process_without_stamp() -> Result<()> {
    let mut c = try!(Context::new(None));
    try!(c.gather_inputs());
    try!(c.generate_content());
    try!(c.check_input_timestamps());
    try!(c.create_stamp());
    try!(c.adjust_stamp_timestamp());
    // try!(c.report_dir(Path::new(".")));
    Ok(())
}

#[derive(Debug)]
struct RsPath(PathBuf);
#[derive(Debug)]
struct MdPath(PathBuf);

struct Context {
    orig_stamp: Option<(File, u64)>,
    src_inputs: Vec<Transform<RsPath, MdPath>>,
    lit_inputs: Vec<Transform<MdPath, RsPath>>,
    newest_stamp: Option<u64>,
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
    if Extensions::extension(p) != Some(ext) { panic!("{t} requires `.{ext}` extension; path: {p:?}", t=typename, ext=ext, p=p); }
    if !p.starts_with(root) { panic!("{t} must be rooted at `{root}/`; path: {p:?}", t=typename, root=root, p=p); }
}

impl RsPath {
    fn new(p: PathBuf) -> RsPath {
        check_path("RsPath", &p, "rs", SRC_DIR);
        RsPath(p)
    }
    fn to_md(&self) -> MdPath {
        let mut p = PathBuf::new();
        p.push(LIT_DIR);
        for c in self.0.components().skip(1) { p.push(c.as_ref().to_str().expect("how else can I replace root?")); }
        p.set_extension("md");
        MdPath::new(p)
    }
}

impl MdPath {
    fn new(p: PathBuf) -> MdPath {
        check_path("MdPath", &p, "md", LIT_DIR);
        MdPath(p)
    }
    fn to_rs(&self) -> RsPath {
        let mut p = PathBuf::new();
        p.push(SRC_DIR);
        for c in self.0.components().skip(1) { p.push(c.as_ref().to_str().expect("how else can I replace root?")); }
        p.set_extension("rs");
        RsPath::new(p)
    }
}

trait Transforms: Sized + Mtime + fmt::Debug {
    type Target: Mtime + fmt::Debug;
    fn target(&self) -> Self::Target;
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
struct Transform<X, Y> {
    source_time: u64,
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
        NoTangoStampExists,
        TangoStampOlderThanTarget { tgt: String },
    }
    #[derive(Debug)]
    pub struct Error(ErrorKind, PathTransform);

    impl fmt::Display for Error {
        fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
            match self.0 {
                ErrorKind::TargetYoungerThanOriginal { ref tgt, ref src } => {
                    write!(w, "target {} is younger than source {}", tgt, src)
                }
                ErrorKind::NoTangoStampExists => {
                    write!(w, "both source and target exist but no `tango.stamp` is present")
                }
                ErrorKind::TangoStampOlderThanTarget { ref tgt } => {
                    write!(w, "`tango.stamp` is older than target {}", tgt)
                }
            }
        }
    }

    impl ErrorTrait for Error {
        fn description(&self) -> &str {
            match self.0 {
                ErrorKind::TargetYoungerThanOriginal { .. }=> {
                    "target is younger than source"
                }
                ErrorKind::NoTangoStampExists => {
                    "both source and target exist but no `tango.stamp` is present"
                }
                ErrorKind::TangoStampOlderThanTarget { .. } => {
                    "`tango.stamp` is older than target"
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
                let mtime = try!(stamp.modified());
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
        if t_mod > s_mod {
            return Ok(TransformNeed::Unneeded);
        } else { // s_mod <= t_mod
            match self.orig_stamp {
                None => return Err(t.error(NoTangoStampExists)),
                Some((_, stamp_time)) => {
                    if stamp_time < t_mod {
                        return Err(t.error(TangoStampOlderThanTarget {
                            tgt: t.generate.display().to_string(),
                        }));
                    }
                }
            }
        }

        // Invariant:
        // Target `t` exists, but,
        // s_mod >= t_mod (and t_mod <= stamp_time if stamp exists).
        //
        // Thus it is safe to overwrite `t` based on source content.
        return Ok(TransformNeed::Needed);
    }

    #[cfg(not_now)]
    fn report_dir(&self, p: &Path) -> Result<()> {
        let src_path = Path::new(SRC_DIR);
        let lit_path = Path::new(LIT_DIR);

        for (i, ent) in try!(fs::walk_dir(p)).enumerate() {
            let ent = try!(ent);
            let modified = try!(ent.modified());
            println!("entry[{}]: {:?} {:?}", i, ent.path(), modified);
        }
        Ok(())
    }

    fn update_newest_time(&mut self, new_time: u64) {
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
        let src_path = Path::new(SRC_DIR);
        let lit_path = Path::new(LIT_DIR);

        fn keep_file_name(p: &Path) -> std::result::Result<(), &'static str> {
            match p.file_name().and_then(|x|x.to_str()) {
                None =>
                    Err("file name is not valid unicode"),
                Some(s) if s.starts_with(".") =>
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

        // println!("gather-rs");
        for ent in try!(fs::walk_dir(src_path)) {
            let ent = try!(ent);
            let p = ent.path();
            if let Err(why) = keep_file_name(p.as_path()) {
                println!("skipping {}; {}", p.display(), why);
                continue;
            }
            if !p.rs_extension() {
                // println!("gather-rs skip {} due to non .rs", p.display());
                continue;
            }
            let rs = RsPath::new(p);
            try!(warn_if_nonexistant(&rs));
            let t = try!(rs.transform());
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
        // println!("gather-md");
        for ent in try!(fs::walk_dir(lit_path)) {
            let ent = try!(ent);
            let p = ent.path();
            if let Err(why) = keep_file_name(p.as_path()) {
                println!("skipping {}; {}", p.display(), why);
                continue;
            }
            if !p.md_extension() {
                // println!("gather-md skip {} due to non .md", p.display());
                continue;
            }
            let md = MdPath::new(p);
            try!(warn_if_nonexistant(&md));
            let t = try!(md.transform());
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
        Ok(())
    }
    fn generate_content(&mut self) -> Result<()> {
        for &Transform { ref original, ref generate, source_time, .. } in &self.src_inputs {
            let source = try!(File::open(&original.0));
            let target = try!(File::create(&generate.0));
            try!(rs2md(source, target));
            try!(fs::set_file_times(&generate.0, source_time, source_time));
        }
        for &Transform { ref original, ref generate, source_time, .. } in &self.lit_inputs {
            let source = try!(File::open(&original.0));
            let target = try!(File::create(&generate.0));
            try!(md2rs(source, target));
            try!(fs::set_file_times(&generate.0, source_time, source_time));
        }
        Ok(())
    }
    fn check_input_timestamps(&mut self) -> Result<()> {
        for &Transform { ref original, source_time, .. } in &self.src_inputs {
            if let MtimeResult::Modified(new_time) = try!(original.modified()) {
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
            if let MtimeResult::Modified(new_time) = try!(original.modified()) {
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
        let _f = try!(File::create(STAMP));
        Ok(())
    }
    fn adjust_stamp_timestamp(&mut self) -> Result<()> {
        if let Some(stamp) = self.newest_stamp {
            match fs::set_file_times(STAMP, stamp, stamp) {
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
    let mut converter = md2rs::Converter::new();
    converter.convert(source, target).map_err(Error::IoError)
}

mod md2rs;

mod rs2md;

#[cfg(test)]
mod testing;

