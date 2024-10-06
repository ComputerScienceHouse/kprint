use actix_web::body::MessageBody;
use futures::future::LocalBoxFuture;
use futures::FutureExt;
use openidconnect::AdditionalClaims;
use std::rc::Rc;
use std::{
    str::FromStr,
    task::{Context, Poll},
};
use uuid::Uuid;

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    FromRequest, HttpMessage, HttpResponse,
};
use openidconnect::{
    core::{
        CoreClient, CoreGenderClaim, CoreJsonWebKeyType, CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm, CoreProviderMetadata,
    },
    reqwest::async_http_client,
    Audience, ClientId, IdToken, IdTokenClaims, IssuerUrl, Nonce, NonceVerifier,
};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

pub struct CSHAuth {
    client: Rc<OnceCell<CoreClient>>,
    client_id: String,
}

impl CSHAuth {
    pub fn new(client_id: String) -> Self {
        CSHAuth {
            client_id,
            client: Rc::new(OnceCell::const_new()),
        }
    }
}

impl<S, ServiceResponseBody> Transform<S, ServiceRequest> for CSHAuth
where
    ServiceResponseBody: MessageBody + 'static,
    S: Service<
            ServiceRequest,
            Response = ServiceResponse<ServiceResponseBody>,
            Error = actix_web::Error,
        > + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = CSHAuthService<S>;
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let client = get_client(self.client_id.clone(), self.client.clone());
        Box::pin(client.map(|client| client.map(|client| CSHAuthService { service, client })))
    }
}

async fn get_client(client_id: String, client: Rc<OnceCell<CoreClient>>) -> Result<CoreClient, ()> {
    client
        .get_or_try_init(|| async move {
            let issuer_url = IssuerUrl::new("https://sso.csh.rit.edu/auth/realms/csh".to_string())
                .expect("Failed to validate issuer URL");
            let provider_metadata =
                CoreProviderMetadata::discover_async(issuer_url, &async_http_client)
                    .await
                    .expect("Failed to get provider metadata");

            // Set up the config for the GitLab OAuth2 process.
            Ok(CoreClient::from_provider_metadata(
                provider_metadata,
                ClientId::new(client_id),
                None,
            ))
        })
        .await
        .cloned()
}

// Please don't use this... I just don't know how computers work :(
struct NullNonceVerifier;
impl NonceVerifier for NullNonceVerifier {
    fn verify(self, _nonce: Option<&Nonce>) -> Result<(), String> {
        Ok(())
    }
}

pub struct CSHAuthService<S> {
    service: S,
    client: CoreClient,
}

impl<S, B> Service<ServiceRequest> for CSHAuthService<S>
where
    B: MessageBody + 'static,
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    #[allow(unused_must_use)]
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let unauthorized = |req: ServiceRequest| -> Self::Future {
            Box::pin(async { Ok(req.into_response(HttpResponse::Unauthorized().finish())) })
        };

        let token = match req.headers().get("Authorization").map(|x| x.to_str()) {
            Some(Ok(x)) => x.trim_start_matches("Bearer ").to_string(),
            _ => {
                log::warn!("Authorization header didn't start with `Bearer`!");
                return unauthorized(req);
            }
        };

        let token = match CshIdToken::from_str(&token) {
            Ok(token) => token,
            Err(err) => {
                log::warn!("Token couldn't be parsed: {err}");
                return unauthorized(req);
            }
        };
        let verifier = self
            .client
            .id_token_verifier()
            .set_other_audience_verifier_fn(|audience| {
                audience == &Audience::new("account".to_owned())
            });
        let claims = match token.into_claims(&verifier, NullNonceVerifier) {
            Ok(claims) => claims,
            Err(err) => {
                log::warn!("Couldn't verify token: {err}");
                return unauthorized(req);
            }
        };

        req.extensions_mut().insert(AuthenticatedUser { claims });

        let future = self.service.call(req);
        Box::pin(async move {
            let response = future.await?;
            Ok(response.map_into_boxed_body())
        })
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CshClaims {
    pub groups: Vec<String>,
    pub uuid: Uuid,
}

pub type CshIdToken = IdToken<
    CshClaims,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
    CoreJsonWebKeyType,
>;

pub type CshIdTokenClaims = IdTokenClaims<CshClaims, CoreGenderClaim>;

impl AdditionalClaims for CshClaims {}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub claims: CshIdTokenClaims,
}

impl FromRequest for AuthenticatedUser {
    type Error = actix_web::error::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let result = match req.extensions().get::<Self>() {
            Some(user) => Ok(user.clone()),
            None => Err(actix_web::error::ErrorUnauthorized("")),
        };
        Box::pin(async { result })
    }
}
