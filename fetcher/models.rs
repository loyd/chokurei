use time::Timespec;

use common::schema::{feed, entry};
use common::types::{Url, Key};

#[derive(Debug, Queryable, Identifiable, AsChangeset)]
#[table_name="feed"]
pub struct Feed {
    pub id: i32,
    pub key: Key,
    pub url: Url,
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub logo: Option<Url>,
    pub copyright: Option<String>,
    pub interval: Option<i32>,
    pub augmented: Option<Timespec>
}

#[derive(Debug, Insertable)]
#[table_name="entry"]
pub struct NewEntry {
    pub key: Key,
    pub feed_id: i32,
    pub url: Option<Url>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub published: Option<Timespec>
}
