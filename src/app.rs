use crate::api::print;
use actix_web::web::{self, scope};
use ipp::prelude::*;
use std::collections::HashMap;

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg.service(scope("/api").service(print));
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
            let mut client_builder =
                AsyncIppClient::builder(Uri::try_from(format!("{cups}/printers/{printer}"))?);
            if let Ok(token) = std::env::var("KPRINT_CUPS_PROXY_TOKEN") {
                client_builder = client_builder.http_header("Authorization", token);
            } else {
                log::warn!("No KPRINT_CUPS_PROXY_TOKEN environment variable was provided! Is your cups server secure?");
            }
            Ok((printer.to_string(), client_builder.build()))
        })
        .collect::<anyhow::Result<HashMap<String, AsyncIppClient>>>()?;

    Ok(AppState { printers })
}
