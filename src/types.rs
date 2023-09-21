//! Shared types and type wrappers with useful trait implementations.

use std::{
    ops::{Deref, Sub},
    str::FromStr,
};

use http::uri::InvalidUri;
use rusqlite::{
    types::{FromSql, FromSqlError},
    ToSql,
};
use sea_query::SimpleExpr;
use serde::{Deserialize, Serialize};
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

impl Into<SimpleExpr> for Time {
    fn into(self) -> SimpleExpr {
        self.format(&DATETIME_FORMAT)
            .expect("Error formatting time")
            .into()
    }
}

impl ToSql for Time {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let time_string: String = self
            .0
            .format(&database::DATETIME_FORMAT)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        Ok(time_string.into())
    }
}

impl FromStr for Time {
    type Err = time::error::Parse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(s, &database::DATETIME_FORMAT).map(Self)
    }
}

impl FromSql for Time {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        OffsetDateTime::parse(value.as_str()?, &database::DATETIME_FORMAT)
            .map_err(|error| FromSqlError::Other(Box::new(error)))
            .map(Time)
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

impl TryFrom<Time> for sea_query::Value {
    type Error = time::Error;

    fn try_from(value: Time) -> Result<Self, Self::Error> {
        let time_string: String = value.format(&database::DATETIME_FORMAT)?;
        Ok(sea_query::Value::String(Some(Box::new(time_string))))
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

impl ToSql for Uri {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let uri_string = self.0.to_string();
        Ok(uri_string.into())
    }
}

impl FromSql for Uri {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|error| FromSqlError::Other(Box::new(error)))
            .map(Self)
    }
}
