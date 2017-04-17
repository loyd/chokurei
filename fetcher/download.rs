use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::str;
use futures::Future;
use tokio_core::reactor::Handle;
use tokio_request::str::get;
use rss::Channel;
use kuchiki::{self, NodeRef};
use kuchiki::traits::TendrilSink;

use common::types::Url;

const USER_AGENT: &str = "Mozilla/5.0 (compatible; chokurei)";

// TODO(loyd): share a session between requests.

pub fn channel(handle: &Handle, url: &Url) -> impl Future<Item=Channel, Error=IoError> + 'static {
    get(url.as_ref())
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|res|
            Channel::read_from(res.body())
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        )
}

pub fn document(handle: &Handle, url: &Url) -> impl Future<Item=NodeRef, Error=IoError> + 'static {
    get(url.as_ref())
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|res|
            str::from_utf8(res.body())
                .map(|body| kuchiki::parse_html().one(body))
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        )
}
