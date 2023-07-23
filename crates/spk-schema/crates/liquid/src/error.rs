use std::error::Error;

use once_cell::sync::Lazy;
use regex::Regex;

#[cfg(test)]
#[path = "./error_test.rs"]
mod error_test;

#[derive(Debug)]
struct ParsedError {
    message: String,
}

impl std::fmt::Display for ParsedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ParsedError {}

pub fn to_error_types(err: liquid::Error) -> format_serde_error::ErrorTypes {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?ms)liquid:  --> (\d+):(\d+)\n.*^\s+= (.*)")
            .expect("a valid regular expression")
    });
    let mut message = err.to_string();
    let mut line = Option::<usize>::None;
    let mut column = Option::<usize>::None;
    if let Some(m) = RE.captures(&message) {
        line = m.get(1).and_then(|line| line.as_str().parse().ok());
        column = m
            .get(2)
            .and_then(|column| column.as_str().parse().ok())
            // format_serde_error appears to use 0-based index for columns
            // whereas the liquid crates uses a 1-based index
            .map(|col: usize| col - 1);
        message = m
            .get(3)
            .map(|msg| msg.as_str())
            .unwrap_or("Invalid Template")
            .trim()
            .to_string();
    }
    let error = Box::new(ParsedError { message });
    format_serde_error::ErrorTypes::Custom {
        error,
        line,
        column,
    }
}
