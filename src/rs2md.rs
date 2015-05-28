use std::io::{self, Read, BufRead, Write};

pub struct Converter { output_state: State, blank_line_count: usize }
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State { MarkdownFirstLine, MarkdownLines, Rust, }
impl Converter {
    pub fn new() -> Converter {
        Converter { output_state: State::MarkdownFirstLine,
                    blank_line_count: 0 }
    }
}

#[derive(Debug)]
enum Effect<'a> {
    BlankLn,
    WriteLn(&'a str),
    StartCodeBlock,
    FinisCodeBlock,
    BlankLitComment,
}

impl Converter {
    pub fn convert<R:io::Read, W:io::Write>(&mut self, r:R, mut w:W) -> io::Result<()> {
        let source = io::BufReader::new(r);
        for line in source.lines() {
            let line = try!(line);
            try!(self.handle(&line, &mut w));
        }
        self.finalize(&mut w)
    }

    pub fn finalize(&mut self, w: &mut Write) -> io::Result<()> {
        match self.output_state {
            State::Rust =>
                self.effect(Effect::FinisCodeBlock, w),
            State::MarkdownFirstLine |
            State::MarkdownLines =>
                Ok(())
        }
    }

    pub fn handle(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        let line_right = line.trim_left();
        if line_right.len() == 0 {
            self.blank_line(w)
        } else if line_right.starts_with("//@ ") {
            let line = &line_right[4..];
            match self.output_state {
                State::Rust =>
                    try!(self.transition(w, State::MarkdownFirstLine)),
                State::MarkdownFirstLine =>
                    try!(self.transition(w, State::MarkdownLines)),
                State::MarkdownLines =>
                    {}
            }
            if line.trim().len() == 0 {
                self.blank_line(w)
            } else {
                self.nonblank_line(line, w)
            }
        } else {
            match self.output_state {
                State::MarkdownFirstLine |
                State::MarkdownLines =>
                    try!(self.transition(w, State::Rust)),
                _ => {}
            }
            self.nonblank_line(line, w)
        }
    }

    fn effect(&mut self, e: Effect, w: &mut Write) -> io::Result<()> {
        match e {
            Effect::BlankLn => writeln!(w, ""),
            Effect::WriteLn(line) => writeln!(w, "{}", line),
            Effect::StartCodeBlock => writeln!(w, "```rust"),
            Effect::FinisCodeBlock => writeln!(w, "```"),
            Effect::BlankLitComment => writeln!(w, "//@"),
        }
    }

    fn nonblank_line(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        for _ in 0..self.blank_line_count {
            try!(self.effect(Effect::BlankLn, w));
        }
        self.blank_line_count = 0;
        self.effect(Effect::WriteLn(line), w)
    }

    fn blank_line(&mut self, _w: &mut Write) -> io::Result<()> {
        self.blank_line_count += 1;
        Ok(())
    }

    fn finish_section(&mut self, _w: &mut Write) -> io::Result<()> {
        Ok(())
    }

    fn transition(&mut self, w: &mut Write, s: State) -> io::Result<()> {
        match s {
            State::MarkdownFirstLine => {
                assert_eq!(self.output_state, State::Rust);
                try!(self.effect(Effect::FinisCodeBlock, w));
                for _ in 0..self.blank_line_count {
                    try!(self.effect(Effect::BlankLn, w));
                }
                self.blank_line_count = 0;
            }
            State::MarkdownLines => {
                assert_eq!(self.output_state, State::MarkdownFirstLine);
                for _ in 0..self.blank_line_count {
                    try!(self.effect(Effect::BlankLitComment, w));
                }
                self.blank_line_count = 0;
            }
            State::Rust => {
                assert!(self.output_state != State::Rust);
                try!(self.finish_section(w));
                for _ in 0..self.blank_line_count {
                    try!(self.effect(Effect::BlankLn, w));
                }
                self.blank_line_count = 0;
                try!(self.effect(Effect::StartCodeBlock, w));
            }
        }
        self.output_state = s;
        Ok(())
    }
}