//! Shared types and type wrappers with useful trait implementations.

use std::{
    ops::{Deref, Sub},
    str::FromStr,
};

use eyre::Context;
use http::uri::InvalidUri;
use serde::{Deserialize, Serialize};
use sqlx::{TypeInfo, Value, ValueRef};
use time::{serde::iso8601, OffsetDateTime};

use crate::{
    database::{self, DATETIME_FORMAT},
    serde::string,
};

/// A Time represnted internally with [OffsetDateTime], which serializes with [iso8601] and
/// is stored in the database using [database::DATETIME_FORMAT].
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Time(#[serde(with = "iso8601")] OffsetDateTime);

impl std::fmt::Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.format(&DATETIME_FORMAT).map_err(|error| {
            tracing::error!("Error formatting time {error}");
            std::fmt::Error
        })?)
    }
}

impl Time {
    /// See [OffsetDateTime].
    pub fn now_utc() -> Self {
        Self(OffsetDateTime::now_utc())
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Time {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        sqlx::Encode::<sqlx::Sqlite>::encode(
            self.format(&DATETIME_FORMAT)
                .expect("Error formatting datetime"),
            buf,
        )
    }
}

impl FromStr for Time {
    type Err = time::error::Parse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(s, &database::DATETIME_FORMAT).map(Self)
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for Time {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        match value.type_info().name() {
            "NUMERIC" | "TEXT" => {
                let value = value.to_owned();
                let s: &str = value.try_decode()?;
                s.parse::<Time>()
                    .wrap_err_with(|| format!("Error parsing time from {s}"))
                    .map_err(Into::into)
            }
            unsupported_type => {
                Err(format!("Unsupported column type for Time: {unsupported_type}").into())
            }
        }
    }
}

impl sqlx::Type<sqlx::Sqlite> for Time {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <OffsetDateTime as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl From<OffsetDateTime> for Time {
    fn from(value: OffsetDateTime) -> Self {
        Self(value)
    }
}

impl Into<OffsetDateTime> for Time {
    fn into(self) -> OffsetDateTime {
        self.0
    }
}

impl Deref for Time {
    type Target = OffsetDateTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Sub<time::Duration> for Time {
    type Output = Self;

    fn sub(self, rhs: time::Duration) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Sub<Time> for Time {
    type Output = time::Duration;

    fn sub(self, rhs: Time) -> Self::Output {
        self.0 - rhs.0
    }
}

#[derive(Hash, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Uri(#[serde(with = "string")] http::Uri);

impl FromStr for Uri {
    type Err = InvalidUri;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl From<http::Uri> for Uri {
    fn from(value: http::Uri) -> Self {
        Self(value)
    }
}

impl Into<http::Uri> for Uri {
    fn into(self) -> http::Uri {
        self.0
    }
}

impl Deref for Uri {
    type Target = http::Uri;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Uri {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        sqlx::Encode::<sqlx::Sqlite>::encode(self.0.to_string(), buf)
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for Uri {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        match value.type_info().name() {
            "TEXT" => {
                let value = value.to_owned();
                let s: &str = value.try_decode()?;
                s.parse::<Uri>().map_err(Into::into)
            }
            unsupported_type => {
                Err(format!("Unsupported column type for Uri: {unsupported_type}").into())
            }
        }
    }
}

/// Represents any value that can be stored in a SQLite database.
#[derive(Debug)]
pub enum AnyValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl std::fmt::Display for AnyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "NULL"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Real(value) => write!(f, "{value}"),
            Self::Text(value) => write!(f, "{value}"),
            Self::Blob(value) => write!(f, "{value:?}"),
        }
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for AnyValue {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        match value.type_info().name() {
            "NULL" => Ok(Self::Null),
            "INTEGER" => Ok(Self::Integer(value.to_owned().try_decode()?)),
            "REAL" => Ok(Self::Real(value.to_owned().try_decode()?)),
            "TEXT" => Ok(Self::Text(value.to_owned().try_decode()?)),
            "BLOB" => Ok(Self::Blob(value.to_owned().try_decode()?)),
            unsupported_type => {
                Err(format!("Unsupported column type for AnyValue: {unsupported_type}").into())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_time_roudtrip() {
        let mut original = Time::now_utc();
        original = Time(original.replace_microsecond(0).unwrap());
        let parsed: Time = original.to_string().parse().unwrap();
        assert_eq!(original, parsed);
    }
}
