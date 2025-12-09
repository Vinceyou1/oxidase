use bytes::Bytes;
use http_body_util::Full;
use hyper::http;

pub fn make_error_resp(status: http::StatusCode, msg: &str) -> http::Response<Full<Bytes>> {
    let mut resp = http::Response::new(Full::from(msg.to_string()));
    *resp.status_mut() = status;
    resp
}
