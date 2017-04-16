use time::Timespec;
use uuid::Uuid;

use key::Key;

#[derive(Debug, Queryable)]
pub struct Feed {
    pub id: i32,
    pub key: Key,
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub logo: Option<String>,
    pub copyright: Option<String>,
    pub interval: Option<i32>,
    pub augmented: Option<Timespec>
}

#[derive(Debug, Queryable)]
pub struct Entry {
    pub id: i64,
    pub key: Key,
    pub feed_id: i32,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub published: Option<Timespec>
}

#[derive(Debug, Queryable)]
pub struct User {
    pub id: i32,
    pub uid: Uuid
}

#[derive(Debug, Queryable)]
pub struct Subscription {
    pub user_id: i32,
    pub feed_id: i32
}
