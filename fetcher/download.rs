use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::str;
use futures::Future;
use tokio_core::reactor::Handle;
use tokio_request::str::get;
use rss::Channel;

use common::types::Url;

const USER_AGENT: &str = "Mozilla/5.0 (compatible; chokurei)";

// TODO(loyd): share a session between requests.
// TODO(loyd): what about redirects?
// TODO(loyd): check http code of responses.

pub fn channel(handle: &Handle, url: &Url) -> impl Future<Item=Channel, Error=IoError> + 'static {
    get(url.as_ref())
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|res|
            Channel::read_from(res.body())
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        )
}

pub fn document(handle: &Handle, url: &Url) -> impl Future<Item=String, Error=IoError> + 'static {
    get(url.as_ref())
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|response| {
            let buffer = Vec::from(response);

            String::from_utf8(buffer)
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        })
}
