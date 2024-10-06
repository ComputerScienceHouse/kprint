use crate::api::print;
use actix_web::web;
use ipp::prelude::*;
use std::collections::HashMap;

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg.service(print);
}

pub struct AppState {
    pub printers: HashMap<String, AsyncIppClient>,
}

pub async fn get_app_data() -> anyhow::Result<AppState> {
    let printers = std::env::var("KPRINT_PRINTERS")
        .expect("No KPRINT_PRINTERS")
        .to_string();
    let printers = printers.split(' ');
    let cups = std::env::var("KPRINT_CUPS_URL")
        .expect("No KPRINT_CUPS_URL")
        .to_string();

    let printers = printers
        .map(|printer| {
            Ok((
                printer.to_string(),
                AsyncIppClient::builder(Uri::try_from(format!("{cups}/printers/{printer}"))?)
                    .http_header(
                        "Authorization",
                        std::env::var("KPRINT_CUPS_PROXY_TOKEN")
                            .expect("No KPRINT_CUPS_PROXY_TOKEN"),
                    )
                    .build(),
            ))
        })
        .collect::<anyhow::Result<HashMap<String, AsyncIppClient>>>()?;

    Ok(AppState { printers })
}
