use serde::Deserialize;

use super::error::ConfigError;

use super::{
    r#static::StaticService,
    router::RouterService,
    forward::ForwardService,
};

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "handler", rename_all = "lowercase")]
pub enum Service {
    Static(StaticService),
    Router(RouterService),
    Forward(ForwardService),
}

pub fn validate_service(svc: &Service) -> Result<(), ConfigError> {
    match svc {
        Service::Static(st) => {
            if st.source_dir.trim().is_empty() {
                return Err(ConfigError::Invalid("`static.source_dir` cannot be empty".into()));
            }
        }
        Service::Router(rt) => {
            if rt.rules.is_empty() {
                return Err(ConfigError::Invalid("`router.rules` cannot be empty".into()));
            }
            validate_service(&rt.next)?;
        }
        Service::Forward(fw) => {
            if fw.target.host.trim().is_empty() {
                return Err(ConfigError::Invalid("`forward.target.host` cannot be empty".into()));
            }
        }
    }
    Ok(())
}
