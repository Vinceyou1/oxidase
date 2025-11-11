pub mod r#static;
pub mod forward;
pub mod router;

use hyper::{body, http};
use http_body_util::Full;
use bytes::Bytes;

use crate::config::Service;

pub trait ServiceHandler {
    fn handle_request(&self, req: &mut http::Request<body::Incoming>) -> http::Response<Full<Bytes>>;
}

impl ServiceHandler for Service {
    fn handle_request(&self, req: &mut http::Request<body::Incoming>) -> http::Response<Full<Bytes>> {
        match self {
            Service::Static(handler) => handler.handle_request(req),
            Service::Forward(handler) => todo!(),
            Service::Router(handler) => todo!(),
        }
    }
}
