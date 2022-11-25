extern crate tango;

use std::error::Error;

pub fn main() {
    tango::process_root().unwrap_or_else(|e| {
        let mut cause: Option<&dyn Error> = Some(&e);
        while let Some(c) = cause {
            let next_cause = c.cause();
            if next_cause.is_some() {
                println!("{}, due to", c.description());
            } else {
                println!("root error: {}", c.description());
            }
            cause = next_cause;
        }
        panic!("IO error {}", e.description());
    })
}
