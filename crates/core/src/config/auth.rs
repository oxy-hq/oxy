use garde::Validate;
use serde::{Deserialize, Serialize};
use std::env;

use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
pub struct Authentication {
    #[garde(dive)]
    pub basic: Option<BasicAuth>,
    #[garde(dive)]
    pub google: Option<GoogleAuth>,
    #[garde(dive)]
    pub okta: Option<OktaAuth>,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
pub struct BasicAuth {
    #[garde(length(min = 1))]
    pub smtp_user: String,
    #[garde(length(min = 1))]
    pub smtp_password: String,

    #[garde(length(min = 1))]
    pub smtp_server: Option<String>,
    #[garde(range(min = 1, max = 65535))]
    pub smtp_port: Option<u16>,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
pub struct GoogleAuth {
    #[garde(length(min = 1))]
    pub client_id: String,
    #[garde(length(min = 1))]
    pub client_secret: String,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
pub struct OktaAuth {
    #[garde(length(min = 1))]
    pub client_id: String,
    #[garde(length(min = 1))]
    pub client_secret: String,
    #[garde(length(min = 1))]
    pub domain: String,
}

impl Authentication {
    pub fn from_env() -> Result<Self, crate::errors::OxyError> {
        let smtp_user = env::var("SMTP_USER").ok();
        let smtp_password = env::var("SMTP_PASSWORD").ok();
        let smtp_server = env::var("SMTP_SERVER").ok();
        let smtp_port = env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok());
        let basic = match (smtp_user, smtp_password) {
            (Some(user), Some(pass)) => Some(BasicAuth {
                smtp_user: user,
                smtp_password: pass,
                smtp_server,
                smtp_port,
            }),
            _ => None,
        };

        let client_id = env::var("GOOGLE_CLIENT_ID").ok();
        let client_secret = env::var("GOOGLE_CLIENT_SECRET").ok();
        let google = match (client_id, client_secret) {
            (Some(id), Some(secret)) => Some(GoogleAuth {
                client_id: id,
                client_secret: secret,
            }),
            _ => None,
        };

        let okta_client_id = env::var("OKTA_CLIENT_ID").ok();
        let okta_client_secret = env::var("OKTA_CLIENT_SECRET").ok();
        let okta_domain = env::var("OKTA_DOMAIN").ok();
        let okta = match (okta_client_id, okta_client_secret, okta_domain) {
            (Some(id), Some(secret), Some(domain)) => Some(OktaAuth {
                client_id: id,
                client_secret: secret,
                domain,
            }),
            _ => None,
        };

        Ok(Authentication {
            basic,
            google,
            okta,
        })
    }
}
