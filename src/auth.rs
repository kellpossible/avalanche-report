use axum::response::IntoResponse;
use base64::Engine;
use http::{HeaderValue, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use tower_http::auth::AuthorizeRequest;

/// Basic authentication for accessing logs.
#[derive(Clone, Copy)]
pub struct MyBasicAuth {
    /// `admin` user password hash, hashed using bcrypt.
    pub admin_password_hash: &'static SecretString,
}

impl<B> AuthorizeRequest<B> for MyBasicAuth {
    type ResponseBody = http_body::combinators::UnsyncBoxBody<axum::body::Bytes, axum::Error>;

    fn authorize(
        &mut self,
        request: &mut axum::http::Request<B>,
    ) -> Result<(), axum::http::Response<Self::ResponseBody>> {
        if check_auth(request, self.admin_password_hash) {
            Ok(())
        } else {
            let unauthorized_response = axum::http::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(
                    "WWW-Authenticate",
                    r#"Basic realm="User Visible Realm", charset="UTF-8""#,
                )
                .body(axum::body::Body::empty())
                .unwrap();

            Err(unauthorized_response.into_response())
        }
    }
}

struct BasicCredentials {
    username: String,
    password: SecretString,
}

fn parse_auth_header_credentials(header: &HeaderValue) -> Option<BasicCredentials> {
    let header_str: &str = header.to_str().ok()?;
    let credentials_base64: &str = header_str.split_once("Basic ")?.1;
    let engine = base64::engine::general_purpose::STANDARD;
    let credentials = String::from_utf8(engine.decode(credentials_base64).ok()?).ok()?;
    let (username, password) = credentials.split_once(':')?;
    Some(BasicCredentials {
        username: username.to_string(),
        password: SecretString::new(password.to_string()),
    })
}

/// Check authorization for a request. Returns `true` if the request is authorized, returns `false` otherwise. Uses Basic http authentication and bcrypt for password hashing.
fn check_auth<B>(
    request: &axum::http::Request<B>,
    admin_password_hash: &'static SecretString,
) -> bool {
    let credentials: BasicCredentials =
        if let Some(auth_header) = request.headers().get("Authorization") {
            if let Some(credentials) = parse_auth_header_credentials(auth_header) {
                credentials
            } else {
                return false;
            }
        } else {
            return false;
        };

    let password_match = bcrypt::verify(
        credentials.password.expose_secret(),
        admin_password_hash.expose_secret(),
    )
    .unwrap_or(false);
    credentials.username == "admin" && password_match
}
