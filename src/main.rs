mod config;
mod handler;
mod http_server;

#[tokio::main]
async fn main() {
    let config
        = config::HttpServer::load_from_file("/Users/weibohan/Downloads/oxidase/config.yaml")
            .expect("Failed to load configuration");
    http_server::start_server(config).await;
}
