use crate::config::error::ConfigError;
use crate::config::forward::ForwardService;
use crate::config::router::RouterService;
use crate::config::service::Service;
use crate::config::r#static::StaticService;
use crate::build::router::{
    LoadedRule,
    compile_rules,
};

const DEFAULT_MAX_STEPS: u32 = 16;

#[derive(Debug, Clone)]
pub enum LoadedService {
    Static(LoadedStatic),
    Router(LoadedRouter),
    Forward(LoadedForward),
}

#[derive(Debug, Clone)]
pub struct LoadedStatic {
    pub config: StaticService,
}

#[derive(Debug, Clone)]
pub struct LoadedForward {
    pub config: ForwardService,
}

#[derive(Debug, Clone)]
pub struct LoadedRouter {
    pub rules: Vec<LoadedRule>,
    pub next: Box<LoadedService>,
    pub max_steps: u32,
}

pub fn build_service(cfg: &Service) -> Result<LoadedService, ConfigError> {
    Ok(match cfg {
        Service::Static(st) => LoadedService::Static(LoadedStatic { config: st.clone() }),
        Service::Forward(fw) => LoadedService::Forward(LoadedForward { config: fw.clone() }),
        Service::Router(rt) => build_router(rt)?,
    })
}

fn build_router(rt: &RouterService) -> Result<LoadedService, ConfigError> {
    let next = build_service(&rt.next)?;
    let max_steps = rt.max_steps.unwrap_or(DEFAULT_MAX_STEPS);

    let rules = compile_rules(&rt.rules)?;

    Ok(LoadedService::Router(LoadedRouter {
        rules,
        next: Box::new(next),
        max_steps,
    }))
}
