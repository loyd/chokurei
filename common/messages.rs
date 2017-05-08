use time::Timespec;
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
pub struct Feed {
    #[serde(with = "url_fmt")]
    pub source: Url,
    #[serde(with = "url_fmt")]
    pub url: Url,
    pub interval: u32,
    #[serde(with = "timespec_fmt")]
    pub augmented: Timespec
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    #[serde(with = "url_fmt")]
    pub source: Url,
    #[serde(with = "url_fmt")]
    pub url: Url,
    pub title: String,
    pub author: Option<String>,
    pub content: String,
    #[serde(with = "timespec_fmt")]
    pub published: Timespec
}

mod timespec_fmt {
    use time::Timespec;
    use serde::{Serializer, Deserializer, Deserialize};

    pub fn serialize<S: Serializer>(timespec: &Timespec, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(timespec.sec as u32)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timespec, D::Error> {
        let ts = u32::deserialize(deserializer)?;

        Ok(Timespec::new(ts as i64, 0))
    }
}

mod url_fmt {
    use url::Url;
    use serde::{Serializer, Deserializer, Deserialize};
    use serde::de::Error as DeError;

    pub fn serialize<S: Serializer>(url: &Url, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(url.as_ref())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Url, D::Error> {
        let url = String::deserialize(deserializer)?;

        Url::parse(&url).map_err(DeError::custom)
    }
}
