use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::str;
use futures::Future;
use tokio_core::reactor::Handle;
use tokio_request::get;
use rss::Channel;
use url::Url;

const USER_AGENT: &str = "Mozilla/5.0 (compatible; chokurei)";

// TODO: share a session between requests.
// TODO: a bad http status code isn't IO error.

pub fn channel(handle: &Handle, url: &Url) -> impl Future<Item=Channel, Error=IoError> + 'static {
    get(url)
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|response| {
            if !response.is_success() {
                let cause = format!("Bad status code: {}", response.status_code());
                return Err(IoError::new(IoErrorKind::Other, cause));
            }

            Channel::read_from(response.body())
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        })
}

pub fn document(handle: &Handle, url: &Url) -> impl Future<Item=String, Error=IoError> + 'static {
    get(url)
        .header("User-Agent", USER_AGENT)
        .send(handle.clone())
        .and_then(|response| {
            if !response.is_success() {
                let cause = format!("Bad status code: {}", response.status_code());
                return Err(IoError::new(IoErrorKind::Other, cause));
            }

            let buffer = Vec::from(response);

            String::from_utf8(buffer)
                .map_err(|cause| IoError::new(IoErrorKind::InvalidData, cause))
        })
}
