use std::io::{self, Read, BufRead, Write};

pub struct Converter {
    state: State,
    blank_line_count: usize,
    buffered_lines: String,
    warnings: Vec<Warning>,
}

use super::Warning;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State { MarkdownBlank, MarkdownText, MarkdownMeta, Rust, }
impl Converter {
    pub fn new() -> Converter {
        Converter {
            state: State::MarkdownBlank,
            blank_line_count: 0,
            buffered_lines: String::new(),
            warnings: vec![],
        }
    }
}

pub enum Exception {
    IoError(io::Error),
    Warnings(Vec<Warning>),
}

impl From<io::Error> for Exception {
    fn from(e: io::Error) -> Self {
        Exception::IoError(e)
    }
}

impl Converter {
    pub fn convert<R:io::Read, W:io::Write>(mut self, r:R, mut w:W) -> Result<(), Exception> {
        let source = io::BufReader::new(r);
        for line in source.lines() {
            let line = try!(line);
            try!(self.handle(&line, &mut w));
        }
        if self.warnings.len() == 0 {
            Ok(())
        } else {
            Err(Exception::Warnings(self.warnings))
        }
    }

    pub fn handle(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        match (self.state, &line.chars().take(7).collect::<String>()[..]) {
            (State::MarkdownBlank, "```rust") |
            (State::MarkdownText, "```rust") => {
                self.buffered_lines = String::new();
                let rest =  &line.chars().skip(7).collect::<String>();
                if rest != "" {
                    try!(self.transition(w, State::MarkdownMeta));
                    try!(self.meta_note(&rest, w));
                }
                self.transition(w, State::Rust)
            }
            (State::Rust, "```") => {
                self.transition(w, State::MarkdownBlank)
            }

            // FIXME: accum blank lines and only emit them with
            // prefix if there's no state transition; otherwise
            // emit them with no prefix. (This is in part the
            // motivation for the `fn finish_section` design.)
            (_, "") => {
                self.blank_line(w)
            }

            _ => {
                // HACK: if we find anything that looks like a markdown-named playpen link ...
                let open_pat = "[";
                let close_pat = "]: https://play.rust-lang.org/?code=";
                if let (Some(open), Some(close)) = (line.find(open_pat), line.find(close_pat)) {
                    // ... then we assume it is associated with the (hopefully immediately preceding)
                    // code block, so we emit a `//@@@` named tag for that code block.

                    // checking here that emitted code block matches
                    // up with emitted url. If non-match, then warn
                    // the user, and suggest they re-run `tango` after
                    // touching the file to generate matching url.
                    let expect = super::encode_to_url(&self.buffered_lines);
                    let actual = &line[(close+3)..];
                    if expect != actual {
                        self.warnings.push(Warning::EncodedUrlMismatch {
                            actual: actual.to_string(),
                            expect: expect
                        })
                    }
                    self.name_block(line, &line[open+1..close], w)
                } else {
                    self.nonblank_line(line, w)
                }
            }
        }
    }

    pub fn meta_note(&mut self, note: &str, w: &mut Write) -> io::Result<()> {
        assert!(note != "");
        self.nonblank_line(note, w)
    }

    pub fn name_block(&mut self, _line: &str, name: &str, w: &mut Write) -> io::Result<()> {
        assert!(name != "");
        writeln!(w, "//@@@ {}", name)
    }

    pub fn nonblank_line(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        let (blank_prefix, line_prefix) = match self.state {
            State::MarkdownBlank => ("", "//@ "),
            State::MarkdownText => ("//@", "//@ "),
            State::MarkdownMeta => ("//@", "//@@"),
            State::Rust => ("", ""),
        };
        for _ in 0..self.blank_line_count {
            try!(writeln!(w, "{}", blank_prefix));

        }
        self.blank_line_count = 0;

        match self.state {
            State::MarkdownBlank =>
                try!(self.transition(w, State::MarkdownText)),
            State::MarkdownMeta => {}
            State::MarkdownText => {}
            State::Rust => {
                self.buffered_lines.push_str("\n");
                self.buffered_lines.push_str(line);
            }
        }

        writeln!(w, "{}{}", line_prefix, line)
    }

    fn blank_line(&mut self, _w: &mut Write) -> io::Result<()> {
        match self.state {
            State::Rust => {
                self.buffered_lines.push_str("\n");
            }
            State::MarkdownBlank |
            State::MarkdownMeta |
            State::MarkdownText => {}
        }
        self.blank_line_count += 1;
        Ok(())
    }

    fn finish_section(&mut self, w: &mut Write) -> io::Result<()> {
        for _ in 0..self.blank_line_count {
            try!(writeln!(w, ""));
        }
        self.blank_line_count = 0;
        Ok(())
    }

    fn transition(&mut self, w: &mut Write, s: State) -> io::Result<()> {
        match s {
            State::MarkdownMeta => {
                assert!(self.state != State::Rust);
                try!(self.finish_section(w));
            }
            State::Rust => {
                assert!(self.state != State::Rust);
                self.buffered_lines = String::new();
            }
            State::MarkdownText => {
                assert_eq!(self.state, State::MarkdownBlank);
                try!(self.finish_section(w));
            }
            State::MarkdownBlank => {
                assert_eq!(self.state, State::Rust);
                try!(self.finish_section(w));
            }
        }
        self.state = s;
        Ok(())
    }
}
