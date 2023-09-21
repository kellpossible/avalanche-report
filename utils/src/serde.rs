pub mod duration_seconds {
    use serde::{Deserializer, Serialize};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<time::Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = time::Duration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting duration in seconds. e.g. 213451")
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(time::Duration::seconds(v.try_into().map_err(|error| {
                    serde::de::Error::custom(format!(
                        "Unable to parse duration as seconds i64: {error}"
                    ))
                })?))
            }
        }

        deserializer.deserialize_u64(Visitor)
    }

    pub fn serialize<S>(duration: &time::Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        duration.whole_seconds().serialize(serializer)
    }
}

pub mod duration_seconds_option {
    use serde::{Deserializer, Serialize};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<time::Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Option<time::Duration>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting duration in seconds or None. e.g. 213451")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Some(time::Duration::seconds(v.try_into().map_err(
                    |error| {
                        serde::de::Error::custom(format!(
                            "Unable to parse duration as seconds i64: {error}"
                        ))
                    },
                )?)))
            }
        }

        deserializer.deserialize_u64(Visitor)
    }

    pub fn serialize<S>(duration: &Option<time::Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        duration
            .map(time::Duration::whole_seconds)
            .serialize(serializer)
    }
}

pub mod rfc3339_option {
    use serde::{Deserializer, Serialize};
    use time::format_description::well_known::Rfc3339;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<time::OffsetDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Option<time::OffsetDateTime>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting duration in seconds or None. e.g. 213451")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Some(
                    time::OffsetDateTime::parse(v, &Rfc3339).map_err(E::custom)?,
                ))
            }
        }

        deserializer.deserialize_str(Visitor)
    }

    pub fn serialize<S>(t: &Option<time::OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(t) = t {
            time::serde::rfc3339::serialize(t, serializer)
        } else {
            Option::<time::OffsetDateTime>::None.serialize(serializer)
        }
    }
}
