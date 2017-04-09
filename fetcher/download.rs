use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::str;
use futures::Future;
use tokio_core::reactor::Core;
use tokio_request::get;
use rss::Channel;
use url::Url;
use kuchiki::{self, NodeRef};
use kuchiki::traits::TendrilSink;

const USER_AGENT: &str = "Mozilla/5.0 (compatible; chokurei)";

//#TODO(loyd): share a session between requests.

pub fn channel<'a>(lp: &Core, url: &Url) -> impl Future<Item=Channel, Error=IoError> + 'a {
    get(url)
        .header("User-Agent", USER_AGENT)
        .send(lp.handle())
        .and_then(|res|
            Channel::read_from(res.body())
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        )
}

pub fn document<'a>(lp: &Core, url: &Url) -> impl Future<Item=NodeRef, Error=IoError> + 'a {
    get(url)
        .header("User-Agent", USER_AGENT)
        .send(lp.handle())
        .and_then(|res|
            str::from_utf8(res.body())
                .map(|body| kuchiki::parse_html().one(body))
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        )
}
