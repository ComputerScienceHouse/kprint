use actix_cors::Cors;
use actix_web::{http, middleware::Logger, App, HttpServer};
use dotenvy::dotenv;

mod api;
mod app;
mod auth;
use app::{configure_app, get_app_data};
use auth::CSHAuth;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();
    HttpServer::new(move || {
        App::new()
            .wrap(CSHAuth::new("kprint".to_string()))
            .wrap(
                Cors::default()
                    .allowed_origin("http://localhost:8081")
                    .allowed_methods(vec!["GET", "POST"])
                    .allowed_headers(vec![
                        http::header::AUTHORIZATION,
                        http::header::ACCEPT,
                        http::header::CONTENT_TYPE,
                    ]),
            )
            .wrap(Logger::new(
                "%a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %T",
            ))
            .configure(configure_app)
            .data_factory(get_app_data)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
