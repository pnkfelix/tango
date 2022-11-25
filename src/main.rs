extern crate tango;

use std::error::Error;

pub fn main() {
    tango::process_root().unwrap_or_else(|e| {
        let mut cause: Option<&dyn Error> = Some(&e);
        while let Some(c) = cause {
            let next_cause = c.source();
            if next_cause.is_some() {
                println!("{}, due to", c);
            } else {
                println!("root error: {}", c);
            }
            cause = next_cause;
        }
        panic!("IO error {}", e);
    })
}
