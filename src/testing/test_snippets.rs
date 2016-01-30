pub const ONE_TEXT_LINE_RS: &'static str = "//@ This is a demo without code.";
pub const ONE_TEXT_LINE_MD: &'static str = "This is a demo without code.";

pub const ONE_RUST_LINE_RS: &'static str = r#"fn main() { println!("one rust line"); }"#;

pub const ONE_RUST_LINE_MD: &'static str = r#"```rust
fn main() { println!("one rust line"); }
```
"#;

pub const HELLO_RS: &'static str = r#"//@ # Hello World
//@ This is a Hello World demo.

// Code started here (at this normal comment)
fn main() { println!("Hello World"); }
//@ And then the text resumes here.
"#;

pub const HELLO_MD: &'static str = r#"# Hello World
This is a Hello World demo.

```rust
// Code started here (at this normal comment)
fn main() { println!("Hello World"); }
```
And then the text resumes here.
"#;

pub const HELLO2_RS: &'static str = r#"//@ # Hello World
//@ This is a second Hello World demo.

// Code started here (at this normal comment)
fn main() { println!("Hello World"); }

//@ And then the text resumes here, after a line break.
"#;

pub const HELLO2_MD: &'static str = r#"# Hello World
This is a second Hello World demo.

```rust
// Code started here (at this normal comment)
fn main() { println!("Hello World"); }
```

And then the text resumes here, after a line break.
"#;

pub const HELLO3_RS: &'static str = r#"

// Code started here (at this normal comment)
fn main() { hello() }

//@ Here is some expository text in the middle
//@ It spans ...
//@ ... multiple lines

// Here is yet more code!
// (and we end with code, not doc)
fn hello() { println!("Hello World"); }
"#;

pub const HELLO3_MD: &'static str = r#"

```rust
// Code started here (at this normal comment)
fn main() { hello() }
```

Here is some expository text in the middle
It spans ...
... multiple lines

```rust
// Here is yet more code!
// (and we end with code, not doc)
fn hello() { println!("Hello World"); }
```
"#;

pub const HELLO4_MD: &'static str = r#"# Hello World
Here is some expository text, but this one ...

... has a gap between its lines.
"#;

pub const HELLO4_RS: &'static str = r#"//@ # Hello World
//@ Here is some expository text, but this one ...
//@
//@ ... has a gap between its lines.
"#;

pub const PRODIGAL5_MD: &'static str = r#"# Hello World
```rust
let code_fragment;
```
	
This looks like it has a nice para break before its starts,
but note the tab
"#;

pub const HARVEST5_RS: &'static str = r#"//@ # Hello World
let code_fragment;
//@ 	
//@ This looks like it has a nice para break before its starts,
//@ but note the tab
"#;

pub const RETURN5_MD: &'static str = r#"# Hello World
```rust
let code_fragment;
```

This looks like it has a nice para break before its starts,
but note the tab
"#;

pub const HELLO6_METADATA_MD: &'static str = r#"# Hello World

```rust { .css_class_metadata }
// The question is, can we preserve the .css_class_metdata
```
"#;

pub const HELLO6_METADATA_RS: &'static str = r#"//@ # Hello World

//@@ { .css_class_metadata }
// The question is, can we preserve the .css_class_metdata
"#;

pub const HELLO7_LINK_TO_PLAY_MD: &'static str = r#"# Hello World

```rust
//
```
[hello7]: https://play.rust-lang.org/?code=//&version=nightly
"#;

pub const HELLO7_LINK_TO_PLAY_RS: &'static str = r#"//@ # Hello World

//
//@@@ hello7
"#;

pub const HELLO8_LINK_TO_PLAY_MD: &'static str = r#"# Hello World

```rust
// Here is some content
fn main() { }
```
[hello8]: https://play.rust-lang.org/?code=//%20Here%20is%20some%20content%0Afn%20main()%20{%20}&version=nightly
"#;

pub const HELLO8_LINK_TO_PLAY_RS: &'static str = r#"//@ # Hello World

// Here is some content
fn main() { }
//@@@ hello8
"#;

pub const HELLO9_LINK_TO_PLAY_MD_WARN: &'static str = r#"# Hello World

```rust
// Here is some content
fn main() { }
```
[hello9]: https://play.rust-lang.org/?code=does_not_match&version=nightly
"#;

pub const HELLO9_LINK_TO_PLAY_RS: &'static str = r#"//@ # Hello World

// Here is some content
fn main() { }
//@@@ hello9
"#;
