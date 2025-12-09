pub mod r#static;
pub mod forward;
pub mod router;

use hyper::{body, http};
use http_body_util::Full;
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;

use crate::config::service::Service;

pub type BoxResponseFuture<'a> = Pin<Box<dyn Future<Output = http::Response<Full<Bytes>>> + Send + 'a>>;

pub trait ServiceHandler {
    fn handle_request<'a>(&'a self, req: &'a mut http::Request<body::Incoming>) -> BoxResponseFuture<'a>;
}

impl ServiceHandler for Service {
    fn handle_request<'a>(&'a self, req: &'a mut http::Request<body::Incoming>) -> BoxResponseFuture<'a> {
        match self {
            Service::Static(handler) => handler.handle_request(req),
            Service::Router(_handler) => todo!(),
            Service::Forward(handler) => handler.handle_request(req),
        }
    }
}
