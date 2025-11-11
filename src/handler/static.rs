pub mod r#static {
    use crate::config::StaticService;
    use crate::handler::ServiceHandler;
    use hyper::{body, http};
    use http_body_util::Full;
    use bytes::Bytes;
    use std::{fs};
    
    impl ServiceHandler for StaticService {
        fn handle_request(&self, req: &mut http::Request<body::Incoming>) -> http::Response<Full<Bytes>> {
            let url_path = req.uri().path();
            eprintln!("[Static] Handling request for: {}", url_path);

            let file_path = format!(
                "{}{}",
                self.source_dir,
                if url_path.ends_with("/") {
                    format!("{}{}", url_path, self.file_index)
                } else {
                    String::from(url_path)
                },
            );
            eprintln!("[Static] Serving static file: {}", file_path);
            
            match fs::read(file_path) {
                Ok(content) => {
                    let mut response = http::Response::new(Full::from(content));
                    *response.status_mut() = http::StatusCode::OK;
                    response
                }
                Err(_) => {
                    let file_path_404 = format!("{}/{}", self.source_dir, self.file_404);
                    eprintln!("[Static] Serving static file: {}", file_path_404);

                    let mut response = http::Response::new(Full::from(
                        match fs::read(file_path_404) {
                            Ok(content_404) => content_404,
                            Err(_) => b"404 Not Found".to_vec(),
                        }
                    ));
                    *response.status_mut() = http::StatusCode::NOT_FOUND;
                    response
                }
            }
        }
    }
}
