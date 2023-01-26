use axum::{http::StatusCode, response::IntoResponse};

pub fn handle_eyre_error(error: eyre::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

pub fn handle_std_error(error: impl std::error::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
