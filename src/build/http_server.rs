use crate::config::error::ConfigError;
use crate::config::http_server::HttpServer;
use crate::build::service::{LoadedService, build_service};

#[derive(Debug, Clone)]
pub struct BuiltHttpServer {
    pub bind: String,
    pub tls: Option<crate::config::tls::TlsConfig>,
    pub service: LoadedService,
}

pub fn build_http_server(cfg: HttpServer) -> Result<BuiltHttpServer, ConfigError> {
    cfg.validate()?;
    let service = build_service(&cfg.service)?;
    Ok(BuiltHttpServer {
        bind: cfg.bind,
        tls: cfg.tls,
        service,
    })
}
