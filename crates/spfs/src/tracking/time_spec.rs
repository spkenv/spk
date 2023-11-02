// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./time_spec_test.rs"]
mod time_spec_test;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = SECONDS_PER_MINUTE * 60;
const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
const SECONDS_PER_WEEK: u64 = SECONDS_PER_DAY * 7;
const SECONDS_PER_YEAR: u64 = SECONDS_PER_DAY * 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimeSpec {
    Relative(Duration),
    Absolute(DateTime<Utc>),
}

impl TimeSpec {
    /// Create a new timespec that repesent the current point in time
    pub fn now() -> Self {
        Self::Absolute(Utc::now())
    }

    /// Provide an absolute datetime for this timespec
    ///
    /// If this spec is a relative time it will be resolved
    /// from the current system time
    pub fn to_datetime_from_now(&self) -> DateTime<Utc> {
        match self {
            Self::Absolute(dt) => *dt,
            Self::Relative(dur) => Utc::now() - *dur,
        }
    }

    /// Provide an absolute datetime for this timespec
    pub fn to_datetime(&self, from: &DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Absolute(dt) => *dt,
            Self::Relative(dur) => *from - *dur,
        }
    }

    /// Create an abolute representation of this spec.
    ///
    /// If this spec is already absolute, this will create a clone.
    pub fn to_abs(&self, from: &DateTime<Utc>) -> Self {
        Self::Absolute(self.to_datetime(from))
    }

    /// Create an abolute representation of this spec.
    ///
    /// If this spec is already absolute, this will create a clone.
    pub fn to_abs_from_now(&self) -> Self {
        Self::Absolute(self.to_datetime_from_now())
    }

    pub fn parse<S: AsRef<str>>(source: S) -> Result<Self> {
        let (prefix, tail) = source.as_ref().split_at(1);
        match prefix {
            "~" => Self::parse_relative_time(tail),
            "@" => Self::parse_absolute_time(tail),
            _ => Err(Error::InvalidTimeSpec {
                given: source.as_ref().to_string(),
                reason: "Must start with either @ or ~ (eg: ~10m, @2020-01-01T10:00:00+04:00)"
                    .to_string(),
            }),
        }
    }

    /// Parse an relative time specifier (without the leading ~ prefix)
    pub fn parse_relative_time<S: AsRef<str>>(source: S) -> Result<Self> {
        let duration = parse_duration(source.as_ref())?;
        Ok(Self::Relative(Duration::from_std(duration).map_err(
            |err| Error::InvalidTimeSpec {
                given: source.as_ref().to_string(),
                reason: err.to_string(),
            },
        )?))
    }

    /// Parse an absolute time specifier (without the leading @ prefix)
    pub fn parse_absolute_time<S: AsRef<str>>(source: S) -> Result<Self> {
        let source = source.as_ref();
        let first_non_digit = source
            .chars()
            .find(|c| !c.is_ascii_digit())
            .ok_or_else(|| Error::InvalidTimeSpec {
                given: source.to_string(),
                reason: "Could not determine how to parse, expected a date (2020-01-31), time (9am, 9:30pm, 14:45) or datetime (2020-010-31T09:45:00+04:00)"
                    .to_string(),
            })?;
        match first_non_digit {
            '-' if source.contains('T') => {
                let dt = DateTime::parse_from_rfc3339(source)
                    .map_err(|err| Error::InvalidTimeSpec {
                        given: source.to_string(),
                        reason: err.to_string(),
                    })?
                    .with_timezone(&Utc);
                Ok(Self::Absolute(dt))
            }
            '-' => {
                let date = chrono::NaiveDate::parse_from_str(source, "%F")
                    .map_err(|err| Error::InvalidTimeSpec {
                        given: source.to_string(),
                        reason: err.to_string(),
                    })?
                    .and_hms_opt(0, 0, 0)
                    .ok_or_else(|| Error::InvalidTimeSpec {
                        given: source.to_string(),
                        reason: "Invalid datetime created".to_string(),
                    })?;
                let dt = chrono::Local.from_local_datetime(&date)
                    .single()
                    .ok_or_else(|| Error::InvalidTimeSpec {
                        given: source.to_string(),
                        reason: "Could not resolve given time unambiguously".to_string(),
                    })?
                    .with_timezone(&Utc);
                Ok(Self::Absolute(dt))
            }
            ':' => {
                let time = parse_time(source)?;
                let datetime = chrono::Local::now().date_naive().and_time(time).and_local_timezone(Utc).single().ok_or_else(|| Error::InvalidTimeSpec {
                    given: source.to_string(),
                        reason: "Could not resolve given time unambiguously".to_string(),
                })?;
                Ok(Self::Absolute(datetime))
            }
            'a' => {
                let digits: String = source.chars().take_while(|c| c.is_ascii_digit()).collect();
                let suffix: String = source.chars().skip(digits.len()).collect();
                let time = parse_time(&format!("{digits}:00{suffix}"))?;
                let datetime = chrono::Local::now().date_naive().and_time(time).and_local_timezone(Utc).single().ok_or_else(|| Error::InvalidTimeSpec {
                    given: source.to_string(),
                        reason: "Could not resolve given time unambiguously".to_string(),
                })?;
                Ok(Self::Absolute(datetime))
            }
            'p' => {
                let digits: String = source.chars().take_while(|c| c.is_ascii_digit()).collect();
                let suffix: String = source.chars().skip(digits.len()).collect();
                let time = parse_time(&format!("{digits}:00{suffix}"))?;
                let datetime = chrono::Local::now().date_naive().and_time(time).and_local_timezone(Utc).single().ok_or_else(|| Error::InvalidTimeSpec {
                    given: source.to_string(),
                        reason: "Could not resolve given time unambiguously".to_string(),
                })?;
                Ok(Self::Absolute(datetime))
            }
            _ => Err(Error::InvalidTimeSpec {
                given: source.to_string(),
                reason: "Expected a date (2020-01-31), time (9am, 9:30pm, 14:45) or datetime (2020-010-31T09:45:00+04:00)"
                    .to_string(),
            }),
        }
    }
}

impl Default for TimeSpec {
    fn default() -> Self {
        Self::Relative(chrono::Duration::zero())
    }
}

impl<T: chrono::TimeZone> From<DateTime<T>> for TimeSpec {
    fn from(dt: DateTime<T>) -> Self {
        Self::Absolute(dt.with_timezone(&Utc))
    }
}

impl std::str::FromStr for TimeSpec {
    type Err = crate::Error;

    fn from_str(string: &str) -> Result<Self> {
        Self::parse(string)
    }
}

impl std::fmt::Display for TimeSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        match self {
            Self::Absolute(dt) => {
                f.write_char('@')?;
                f.write_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
            }
            Self::Relative(dur) => {
                f.write_char('~')?;
                match dur.num_seconds() as u64 {
                    secs if secs % SECONDS_PER_YEAR == 0 => {
                        f.write_str(&format!("{}y", secs / SECONDS_PER_YEAR))
                    }
                    secs if secs % SECONDS_PER_WEEK == 0 => {
                        f.write_str(&format!("{}w", secs / SECONDS_PER_WEEK))
                    }
                    secs if secs % SECONDS_PER_DAY == 0 => {
                        f.write_str(&format!("{}d", secs / SECONDS_PER_DAY))
                    }
                    secs if secs % SECONDS_PER_HOUR == 0 => {
                        f.write_str(&format!("{}h", secs / SECONDS_PER_HOUR))
                    }
                    secs if secs % SECONDS_PER_MINUTE == 0 => {
                        f.write_str(&format!("{}m", secs / SECONDS_PER_MINUTE))
                    }
                    secs => f.write_str(&format!("{secs}s")),
                }
            }
        }
    }
}

pub fn parse_time(source: &str) -> Result<chrono::NaiveTime> {
    const VALID_PATTERNS: &[&str] = &["%l:%M%P", "%k:%M"];
    for pattern in VALID_PATTERNS {
        if let Ok(t) = chrono::NaiveTime::parse_from_str(source, pattern) {
            return Ok(t);
        }
    }
    Err(Error::InvalidTimeSpec {
        given: source.to_string(),
        reason: "Could not parse as a valid time (9:00am, 9:30pm, 14:45)".to_string(),
    })
}

pub fn parse_duration<S: AsRef<str>>(source: S) -> Result<std::time::Duration> {
    let source = source.as_ref();
    let digits: String = source.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix: String = source.chars().skip(digits.len()).collect();
    let number = digits
        .parse::<u64>()
        .map_err(|err| Error::InvalidTimeSpec {
            given: source.to_string(),
            reason: format!("Failed to parse relative time as number: {err:?}"),
        })?;

    let duration = match suffix.as_str() {
            "y" | "year" | "years" => std::time::Duration::from_secs(number * SECONDS_PER_YEAR),
            "w" | "week" | "weeks" => std::time::Duration::from_secs(number * SECONDS_PER_WEEK),
            "d" | "day" | "days" => std::time::Duration::from_secs(number * SECONDS_PER_DAY),
            "h" | "hour" | "hours" => std::time::Duration::from_secs(number * SECONDS_PER_HOUR),
            "m" | "minute" | "minutes" => std::time::Duration::from_secs(number * SECONDS_PER_MINUTE),
            "s" | "second" | "seconds" => std::time::Duration::from_secs(number),
            _ => return Err(Error::InvalidTimeSpec {
                given: source.to_string(),
                reason: format!("Unknown time unit '{suffix}', expected one of: y(ears), w(eeks), d(ays), h(ours), m(inutes), s(econds)"),
            })
        };
    Ok(duration)
}

impl<'de> serde::de::Deserialize<'de> for TimeSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TimeSpecVisitor;

        impl<'de> serde::de::Visitor<'de> for TimeSpecVisitor {
            type Value = TimeSpec;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a relative or absolute time specificer (eg: ~10m, @2022-01-01)")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                TimeSpec::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(TimeSpecVisitor)
    }
}

impl serde::ser::Serialize for TimeSpec {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
