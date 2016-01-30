pub use self::ConverterArgs::{LineForLine, UseFootnotes};

use std::io::{self, Read, BufRead, Write};

pub enum ConverterArgs {
    LineForLine,
    UseFootnotes,
}

pub struct Converter { args: ConverterArgs, state: State, blank_line_count: usize }
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State { MarkdownBlank, MarkdownText, MarkdownMeta, Rust, }
impl Converter {
    pub fn new(args: ConverterArgs) -> Converter {
        Converter { args: args, state: State::MarkdownBlank, blank_line_count: 0 }
    }
}

impl Converter {
    pub fn convert<R:io::Read, W:io::Write>(&mut self, r:R, mut w:W) -> io::Result<()> {
        let source = io::BufReader::new(r);
        for line in source.lines() {
            let line = try!(line);
            try!(self.handle(&line, &mut w));
        }
        Ok(())
    }

    pub fn handle(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        match (self.state, &line.chars().take(7).collect::<String>()[..]) {
            (State::MarkdownBlank, "```rust") |
            (State::MarkdownText, "```rust") => {
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
                self.nonblank_line(line, w)
            }
        }
    }

    pub fn meta_note(&mut self, note: &str, w: &mut Write) -> io::Result<()> {
        assert!(note != "");
        self.nonblank_line(note, w)
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
            State::Rust => {}
        }

        writeln!(w, "{}{}", line_prefix, line)
    }

    fn blank_line(&mut self, _w: &mut Write) -> io::Result<()> {
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
