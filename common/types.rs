use std::fmt;
use std::io::Write;
use std::error::Error;

use url;
use diesel::row::Row;
use diesel::backend::Backend;
use diesel::types::{Text, Nullable, IsNull, FromSqlRow, ToSql};
use diesel::expression::AsExpression;
use diesel::expression::helper_types::AsExprOf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url(url::Url);

impl Url {
    pub fn parse(input: &str) -> Result<Url, ()> {
        url::Url::parse(input).map(Url).map_err(|_| ())
    }
}

impl<DB> FromSqlRow<Text, DB> for Url
    where DB: Backend<RawValue=[u8]>
{
    fn build_from_row<R: Row<DB>>(row: &mut R) -> Result<Url, Box<Error + Send + Sync>> {
        let string = String::build_from_row(row)?;

        url::Url::parse(&string).map(Url).map_err(From::from)
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Into<String> for Url {
    fn into(self) -> String {
        self.0.into_string()
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

impl<'a> AsExpression<Nullable<Text>> for &'a Url {
    type Expression = AsExprOf<String, Nullable<Text>>;

    fn as_expression(self) -> Self::Expression {
        AsExpression::<Nullable<Text>>::as_expression(self.0.as_str().to_owned())
    }
}

impl<'a> AsExpression<Text> for &'a Url {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        AsExpression::<Text>::as_expression(self.0.as_str().to_owned())
    }
}

impl<DB> ToSql<Text, DB> for Url
    where DB: Backend,
          for<'a> &'a str: ToSql<Text, DB>
{
    fn to_sql<W: Write>(&self, out: &mut W) -> Result<IsNull, Box<Error + Send + Sync>> {
        self.0.as_str().to_sql(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(String);

impl From<Url> for Key {
    fn from(url: Url) -> Key {
        let url = url.0;

        let host = url.host_str().map(|host| host.trim_left_matches("www."));
        let port = url.port();
        let mut path = url.path().trim_right_matches("/").to_lowercase();

        while path.contains("//") {
            path = path.replace("//", "/");
        }

        let mut value = String::with_capacity(host.map_or(0, |h| h.len())
                                              + if port.is_some() { 6 } else { 0 }
                                              + path.len());
        if let Some(host) = host {
            value.push_str(host);
        }

        if let Some(port) = port {
            value.push_str(":");
            value.push_str(&port.to_string());
        }

        value.push_str(&path);

        Key(value)
    }
}

impl From<Uuid> for Key {
    fn from(uuid: Uuid) -> Key {
        Key(uuid.simple().to_string())
    }
}

impl<DB> FromSqlRow<Text, DB> for Key
    where DB: Backend<RawValue=[u8]>
{
    fn build_from_row<R: Row<DB>>(row: &mut R) -> Result<Key, Box<Error + Send + Sync>> {
        String::build_from_row(row).map(Key)
    }
}

impl AsRef<str> for Key {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Into<String> for Key {
    fn into(self) -> String {
        self.0
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.write_str(&self.0)
    }
}

impl<'a> AsExpression<Nullable<Text>> for &'a Key {
    type Expression = AsExprOf<String, Nullable<Text>>;

    fn as_expression(self) -> Self::Expression {
        AsExpression::<Nullable<Text>>::as_expression(self.0.clone())
    }
}

impl<'a> AsExpression<Text> for &'a Key {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        AsExpression::<Text>::as_expression(self.0.clone())
    }
}

macro_rules! test {
    ($source:expr, $expected:expr) => {
        let url = Url::parse($source).unwrap();
        assert_eq!(Key::from(url).0, $expected);
    }
}

#[test]
fn it_removes_schema() {
    test!("http://example.com", "example.com");
}

#[test]
fn it_lowercases_all() {
    test!("HTTP://Example.COM", "example.com");
    test!("https://example.com/Test/Foo/bAr.HtMl", "example.com/test/foo/bar.html");
}

#[test]
fn it_removes_default_port() {
    test!("http://example.com:80", "example.com");
    test!("http://example.com:88", "example.com:88");
    test!("https://example.com:443", "example.com");
    test!("https://example.com:442", "example.com:442");
}

#[test]
fn it_removes_trailing_slash() {
    test!("https://example.com/", "example.com");
    test!("https://example.com/test/", "example.com/test");
}

#[test]
fn it_removes_www() {
    test!("http://www.example.com", "example.com");
}

#[test]
fn it_resolves_pathes() {
    test!("https://example.com/test//foo.html", "example.com/test/foo.html");
}

#[test]
fn it_remove_multiple_slashes() {
    test!("http://example.com/one//two///three////four", "example.com/one/two/three/four");
}

#[test]
fn it_removes_hashes() {
    test!("https://example.com#test", "example.com");
    test!("https://example.com/test#test", "example.com/test");
    test!("https://example.com/test.html/#test", "example.com/test.html");
}
