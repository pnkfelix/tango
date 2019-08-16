use super::encode_to_url;
use std::io::{self, BufRead, Write};

#[derive(Debug)]
pub struct Converter {
    output_state: State,
    blank_line_count: usize,
    buffered_code: String,
    meta_note: Option<String>,
}
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State {
    MarkdownFirstLine,
    MarkdownLines,
    Rust,
}
impl Converter {
    pub fn new() -> Converter {
        Converter {
            output_state: State::MarkdownFirstLine,
            blank_line_count: 0,
            buffered_code: String::new(),
            meta_note: None,
        }
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

#[derive(Debug)]
enum EffectContext<'a> {
    Finalize,
    NonblankLine(&'a str),
    Transition(State),
}

impl Converter {
    pub fn convert<R: io::Read, W: io::Write>(&mut self, r: R, mut w: W) -> io::Result<()> {
        let source = io::BufReader::new(r);
        for line in source.lines() {
            let line = try!(line);
            try!(self.handle(&line, &mut w));
        }
        self.finalize(&mut w)
    }

    pub fn finalize(&mut self, w: &mut Write) -> io::Result<()> {
        match self.output_state {
            State::Rust => self.effect(EffectContext::Finalize, Effect::FinisCodeBlock, w),
            State::MarkdownFirstLine | State::MarkdownLines => Ok(()),
        }
    }

    pub fn handle(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        let line_right = line.trim_left();
        if line_right.is_empty() {
            self.blank_line(w)
        } else if line_right.starts_with("//@ ") {
            let line = &line_right[4..];
            if line.trim().is_empty() {
                try!(self.blank_line(w))
            }
            match self.output_state {
                State::Rust => try!(self.transition(w, State::MarkdownFirstLine)),
                State::MarkdownFirstLine => try!(self.transition(w, State::MarkdownLines)),
                State::MarkdownLines => {}
            }
            if line.trim().is_empty() {
                Ok(())
            } else {
                self.nonblank_line(line, w)
            }
        } else if line_right.starts_with("//@@@") {
            let line = &line_right[5..];
            if !line.trim().is_empty() {
                match self.output_state {
                    State::Rust => {
                        try!(self.transition(w, State::MarkdownFirstLine));
                        try!(self.emit_named_code(line.trim(), w));
                    }
                    State::MarkdownFirstLine => {
                        try!(self.transition(w, State::MarkdownLines));
                        try!(self.emit_named_code(line.trim(), w));
                    }
                    State::MarkdownLines => {
                        try!(self.emit_named_code(line.trim(), w));
                    }
                }
            }
            Ok(())
        } else if line_right.starts_with("//@@") {
            let line = &line_right[4..];
            if !line.trim().is_empty() {
                self.set_meta_note(line.trim());
            }
            Ok(())
        } else if line_right.starts_with("//@") {
            let line = &line_right[3..];
            match self.output_state {
                State::Rust => try!(self.transition(w, State::MarkdownFirstLine)),
                State::MarkdownFirstLine => try!(self.transition(w, State::MarkdownLines)),
                State::MarkdownLines => {}
            }
            if line.trim().is_empty() {
                self.blank_line(w)
            } else {
                self.nonblank_line(line, w)
            }
        } else {
            match self.output_state {
                State::MarkdownFirstLine | State::MarkdownLines => {
                    try!(self.transition(w, State::Rust))
                }
                _ => {}
            }
            self.nonblank_line(line, w)
        }
    }

    fn emit_named_code(&mut self, name: &str, w: &mut Write) -> io::Result<()> {
        writeln!(w, "[{}]: {}", name, encode_to_url(&self.buffered_code))
    }

    fn set_meta_note(&mut self, note: &str) {
        if let Some(ref prev_note) = self.meta_note {
            println!("warning: discarding meta note {} for {}", prev_note, note);
        }
        self.meta_note = Some(note.to_string());
    }

    fn effect(&mut self, _c: EffectContext, e: Effect, w: &mut Write) -> io::Result<()> {
        // println!("effect _c: {:?} e: {:?}", _c, e);
        match e {
            Effect::BlankLn => writeln!(w, ""),
            Effect::WriteLn(line) => writeln!(w, "{}", line),
            Effect::StartCodeBlock => {
                if let Some(ref note) = self.meta_note {
                    assert_eq!(note.chars().next(), Some('{'));
                    try!(writeln!(w, "```{{.rust{}", &note[1..]));
                } else {
                    try!(writeln!(w, "```rust"));
                }
                self.meta_note = None;
                self.buffered_code = String::new();
                Ok(())
            }
            Effect::FinisCodeBlock => {
                try!(writeln!(w, "```"));
                Ok(())
            }
            Effect::BlankLitComment => writeln!(w, ""),
        }
    }

    fn nonblank_line(&mut self, line: &str, w: &mut Write) -> io::Result<()> {
        for _ in 0..self.blank_line_count {
            try!(self.effect(EffectContext::NonblankLine(line), Effect::BlankLn, w));
        }
        if State::Rust == self.output_state {
            self.buffered_code = format!("{}\n{}", self.buffered_code, line);
        }
        self.blank_line_count = 0;
        self.effect(EffectContext::NonblankLine(line), Effect::WriteLn(line), w)
    }

    fn blank_line(&mut self, _w: &mut Write) -> io::Result<()> {
        self.blank_line_count += 1;
        if State::Rust == self.output_state {
            self.buffered_code = format!("{}\n", self.buffered_code);
        }
        Ok(())
    }

    fn finish_section(&mut self, _w: &mut Write) -> io::Result<()> {
        Ok(())
    }

    fn transition(&mut self, w: &mut Write, s: State) -> io::Result<()> {
        match s {
            State::MarkdownFirstLine => {
                assert_eq!(self.output_state, State::Rust);
                try!(self.effect(EffectContext::Transition(s), Effect::FinisCodeBlock, w));
                for _ in 0..self.blank_line_count {
                    try!(self.effect(EffectContext::Transition(s), Effect::BlankLn, w));
                }
                self.blank_line_count = 0;
            }
            State::MarkdownLines => {
                assert_eq!(self.output_state, State::MarkdownFirstLine);
                for _ in 0..self.blank_line_count {
                    try!(self.effect(EffectContext::Transition(s), Effect::BlankLitComment, w));
                }
                self.blank_line_count = 0;
            }
            State::Rust => {
                assert!(self.output_state != State::Rust);
                try!(self.finish_section(w));
                for _ in 0..self.blank_line_count {
                    try!(self.effect(EffectContext::Transition(s), Effect::BlankLn, w));
                }
                self.blank_line_count = 0;
                try!(self.effect(EffectContext::Transition(s), Effect::StartCodeBlock, w));
            }
        }
        self.output_state = s;
        Ok(())
    }
}
