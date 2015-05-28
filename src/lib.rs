#![feature(dir_entry_ext, fs_time, fs_walk, path_ext, file_path)]

use std::convert;
use std::env;
use std::error::Error as ErrorTrait;
use std::fmt;
use std::fs::{self, File, PathExt};
use std::io::{self, Read, BufRead, BufReader, Write};
use std::ops;
use std::path::{Path, PathBuf};
use std::result;

pub const STAMP: &'static str = "tango.stamp";
pub const SRC: &'static str = "src";
pub const LIT: &'static str = "lit";

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
    let root = try!(env::current_dir());
    // println!("Tango is running from: {:?}", root);
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
        check_path("RsPath", &p, "rs", "src");
        RsPath(p)
    }
    fn to_md(&self) -> MdPath {
        let mut p = PathBuf::new();
        p.push("lit");
        for c in self.0.components().skip(1) { p.push(c.as_ref().to_str().expect("how else can I replace root?")); }
        p.set_extension("md");
        MdPath::new(p)
    }
}

impl MdPath {
    fn new(p: PathBuf) -> MdPath {
        check_path("MdPath", &p, "md", "lit");
        MdPath(p)
    }
    fn to_rs(&self) -> RsPath {
        let mut p = PathBuf::new();
        p.push("src");
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
            Ok(MtimeResult::NonExistant) => panic!("impossible"),
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
        TargetYoungerThanOriginal,
        NoTangoStampExists,
        TangoStampNotOlderThanTarget,
    }
    #[derive(Debug)]
    pub struct Error(ErrorKind, PathTransform);

    impl fmt::Display for Error {
        fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
            let s = match self.0 {
                ErrorKind::TargetYoungerThanOriginal => {
                    "target is younger than source"
                }
                ErrorKind::NoTangoStampExists => {
                    "both source and target exist but no `tango.stamp` is present"
                }
                ErrorKind::TangoStampNotOlderThanTarget => {
                    "target is younger than `tango.stamp`"
                }
            };
            write!(w, "{}", s)
        }
    }

    impl ErrorTrait for Error {
        fn description(&self) -> &str {
            match self.0 {
                ErrorKind::TargetYoungerThanOriginal => {
                    "target is younger than source"
                }
                ErrorKind::NoTangoStampExists => {
                    "both source and target exist but no `tango.stamp` is present"
                }
                ErrorKind::TangoStampNotOlderThanTarget => {
                    "target is younger than `tango.stamp`"
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

    fn check_transform<X, Y>(&self, t: &Transform<X, Y>) -> check::Result<()>
        where X: ops::Deref<Target=Path> + Mtime,
              Y: ops::Deref<Target=Path> + Mtime,
    {
        use self::check::ErrorKind::*;
        use self::check::Error;
        let to_path_transform = move || {
            Transform { original: t.original.to_path_buf(),
                        generate: t.generate.to_path_buf(),
                        source_time: t.source_time,
                        target_time: t.target_time,
            }
        };
        let t_mod = match t.target_time {
            MtimeResult::Modified(t) => t,
            MtimeResult::NonExistant => {
                assert!(!t.generate.exists());
                return Ok(());
            }
        };
        let g_mod = t.source_time;
        if g_mod > t_mod {
            return Err(t.error(TargetYoungerThanOriginal));
        } else { // g_mod <= t_mod
            match self.orig_stamp {
                None => return Err(t.error(NoTangoStampExists)),
                Some((_, stamp_time)) => {
                    if stamp_time <= g_mod {
                        return Err(t.error(TangoStampNotOlderThanTarget));
                    }
                }
            }
        }

        // Invariant:
        // Target `g` exists, but,
        // g_mod <= t_mod (and g_mod < stamp_time if stamp exists).
        //
        // Thus it is safe to overwrite `g` based on source content.
        Ok(())
    }

    fn report_dir(&self, p: &Path) -> Result<()> {
        let src_path = Path::new(SRC);
        let lit_path = Path::new(LIT);

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
        let src_path = Path::new(SRC);
        let lit_path = Path::new(LIT);

        for (i, ent) in try!(fs::walk_dir(src_path)).enumerate() {
            let ent = try!(ent);
            let p = ent.path();
            if !p.rs_extension() {
                continue;
            }
            let rs = RsPath::new(p);
            let t = try!(rs.transform());
            match self.check_transform(&t) {
                Ok(()) => self.push_src(t),
                Err(e) => {
                    println!("gather_inputs err: {}", e.description());
                    return Err(Error::CheckInputError {
                        error: e,
                    })
                }
            }
        }
        for (i, ent) in try!(fs::walk_dir(lit_path)).enumerate() {
            let ent = try!(ent);
            let p = ent.path();
            if !p.md_extension() {
                continue;
            }
            let md = MdPath::new(p);
            let t = try!(md.transform());
            match self.check_transform(&t) {
                Ok(()) => self.push_lit(t),
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
        for &Transform { ref original, ref generate, .. } in &self.src_inputs {
            let source = try!(File::open(&original.0));
            let target = try!(File::create(&generate.0));
            try!(rs2md(source, target));
        }
        for &Transform { ref original, ref generate, .. } in &self.lit_inputs {
            let source = try!(File::open(&original.0));
            let target = try!(File::create(&generate.0));
            try!(md2rs(source, target));
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

fn rs2md<R:Read, W:Write>(source: R, mut target: W) -> Result<()> {
    let mut converter = rs2md::Converter::new();
    converter.convert(source, target).map_err(Error::IoError)
}

fn md2rs<R:Read, W:Write>(source: R, mut target: W) -> Result<()> {
    let mut converter = md2rs::Converter::new();
    converter.convert(source, target).map_err(Error::IoError)
}

mod md2rs;

mod rs2md;

mod test_snippets;

struct DifferingLines<'a> {
    left_line_num: usize,
    left: &'a str,
    right_line_num: usize,
    right: &'a str,
}

enum ComparisonResult<'a> {
    Ok,
    LineDifferences(Vec<DifferingLines<'a>>),
    LineCountMismatch(usize, usize, Vec<String>),
}

// #[cfg(test)]
fn compare_lines<'a>(a: &'a str, b: &'a str) -> ComparisonResult<'a> {
    let mut a: Vec<_> = a.lines().collect();
    let mut b: Vec<_> = b.lines().collect();
    let mut i = 0;
    let mut j = 0;

    let mut differing_lines: Vec<DifferingLines> = Vec::new();

    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }

        differing_lines.push(DifferingLines {
            left_line_num: i,
            right_line_num: j,
            left: a[i],
            right: b[j],
        });

        for j_ in (j+1)..b.len() {
            if a[i] == b[j_] {
                j = j_;
                continue;
            }
        }

        for i_ in (i+1)..a.len() {
            if a[i_] == b[j] {
                i = i_;
                continue;
            }
        }

        i += 1;
        j += 1;
    }

    if differing_lines.len() != 0 {
        ComparisonResult::LineDifferences(differing_lines)
    } else if i == a.len() && j == b.len() && i == j {
        ComparisonResult::Ok
    } else {
        let mut v = Vec::new();
        if a.len() > b.len() {
            for i in b.len()..a.len() {
                v.push(a[i].to_string());
            }
        } else {
            for j in a.len()..b.len() {
                v.push(b[j].to_string());
            }
        }
        ComparisonResult::LineCountMismatch(a.len(), b.len(), v)
    }
}

fn panic_if_different<'a>(name_a: &str, a: &'a str, name_b: &str, b: &'a str) {
    match compare_lines(a, b) {
        ComparisonResult::LineDifferences(differences) => {
            for difference in differences {
                println!("lines {lnum} and {rnum} differ:\n{nl:>8}: {l}\n{nr:>8}: {r}",
                         lnum=difference.left_line_num+1,
                         rnum=difference.right_line_num+1,
                         nl=name_a,
                         l=difference.left,
                         nr=name_b,
                         r=difference.right);
            }
            panic!("saw differences");
        }
        ComparisonResult::LineCountMismatch(a, b, v) => {
            for line in v {
                println!("excess line: {}", line);
            }
            panic!("Content differs:\n{nl:>8}: {l} lines\n{nr:>8}: {r} lines",
                     nl=name_a,
                     l=a,
                     nr=name_b,
                     r=b);
        }
        ComparisonResult::Ok => {}
    }
}

#[cfg(test)]
fn core_test_md2rs(md: &str, rs: &str) {
    let mut output = Vec::new();
    md2rs(md.as_bytes(), &mut output);
    let output = String::from_utf8(output).unwrap();
    panic_if_different("actual", &output, "expect", rs);
}

#[cfg(test)]
fn core_test_rs2md(rs: &str, md: &str) {
    let mut output = Vec::new();
    rs2md(rs.as_bytes(), &mut output);
    let output = String::from_utf8(output).unwrap();
    panic_if_different("actual", &output, "expect", md);
}

#[test]
fn test_onetext_md2rs() {
    core_test_md2rs(test_snippets::ONE_TEXT_LINE_MD,
                    test_snippets::ONE_TEXT_LINE_RS);
}

#[test]
fn test_onetext_rs2md() {
    core_test_rs2md(test_snippets::ONE_TEXT_LINE_RS,
                    test_snippets::ONE_TEXT_LINE_MD);
}

#[test]
fn test_hello_md2rs() {
    core_test_md2rs(test_snippets::HELLO_MD, test_snippets::HELLO_RS);
}

#[test]
fn test_hello_rs2md() {
    core_test_rs2md(test_snippets::HELLO_RS, test_snippets::HELLO_MD);
}

#[test]
fn test_hello2_md2rs() {
    core_test_md2rs(test_snippets::HELLO2_MD, test_snippets::HELLO2_RS);
}

#[test]
fn test_hello2_rs2md() {
    core_test_rs2md(test_snippets::HELLO2_RS, test_snippets::HELLO2_MD);
}

#[test]
fn test_hello3_md2rs() {
    core_test_md2rs(test_snippets::HELLO3_MD, test_snippets::HELLO3_RS);
}

#[test]
fn test_hello3_rs2md() {
    core_test_rs2md(test_snippets::HELLO3_RS, test_snippets::HELLO3_MD);
}
