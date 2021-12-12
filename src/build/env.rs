// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};

use once_cell::sync::OnceCell;
use regex::Regex;

use crate::{api, Error, Result};

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

static VAR_EXPANSION_REGEX: OnceCell<Regex> = OnceCell::new();

/// Returns the directory that contains package metadata
///
/// This directory is included as part of the package itself, and
/// nearly always has a prefix of /spfs
pub fn data_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    prefix
        .as_ref()
        .join("spk")
        .join("pkg")
        .join(pkg.to_string())
}

/// Expand variables in 'value' with 'vars'.
///
/// Expansions should be in the form of $var and ${var}.
/// Undefined variables are left unchanged.
pub fn expand_defined_vars<V>(mut value: String, vars: V) -> String
where
    V: Fn(&str) -> Option<String>,
{
    if !value.contains('$') {
        return value;
    }
    let var_expansion_regex =
        VAR_EXPANSION_REGEX.get_or_init(|| Regex::new(r"\$(\w+|\{[^}]*\})").unwrap());
    let start = '{';
    let end = '}';
    let mut i = 0;
    loop {
        value = {
            let m = match var_expansion_regex.captures(&value[i..]) {
                Some(m) => m,
                None => break,
            };
            let group = m.get(1).unwrap();
            let mut span = m.get(0).unwrap().range();
            span.start += i;
            span.end += i;
            let mut name = group.as_str();
            if name.starts_with(start) && name.ends_with(end) {
                name = &name[1..name.len() - 1];
            }
            let var = match vars(name) {
                None => {
                    i = span.end;
                    continue;
                }
                Some(var) => {
                    i = span.start + var.len();
                    var
                }
            };
            value
                .chars()
                .take(span.start)
                .chain(var.chars())
                .chain(value.chars().skip(span.end))
                .collect()
        };
    }
    value
}

/// Expand variables in 'value' with 'vars'.1
///
/// Expansions should be in the form of $var and ${var}.
/// Unknown variables raise a KeyError.
pub fn expand_vars<V>(mut value: String, vars: V) -> Result<String>
where
    V: Fn(&str) -> Option<String>,
{
    if !value.contains('$') {
        return Ok(value);
    }
    let var_expansion_regex =
        VAR_EXPANSION_REGEX.get_or_init(|| regex::Regex::new(r"\$(\w+|\{[^}]*\})").unwrap());
    let start = '{';
    let end = '}';
    let mut i = 0;
    loop {
        value = {
            let m = match var_expansion_regex.captures(&value[i..]) {
                Some(m) => m,
                None => break,
            };
            let group = m.get(1).unwrap();
            let mut span = m.get(0).unwrap().range();
            span.start += i;
            span.end += i;
            let mut name = group.as_str();
            if name.starts_with(start) && name.ends_with(end) {
                name = &name[1..name.len() - 1];
            }
            let var = match vars(name) {
                None => {
                    return Err(Error::String(format!(
                        "Undefined variable in string expansion: {}",
                        name
                    )));
                }
                Some(var) => {
                    i = span.start + var.len();
                    var
                }
            };
            value
                .chars()
                .take(span.start)
                .chain(var.chars())
                .chain(value.chars().skip(span.end))
                .collect()
        };
    }
    Ok(value)
}
