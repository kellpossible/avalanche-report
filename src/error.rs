use axum::{http::StatusCode, response::{IntoResponse, Html}};

pub fn map_eyre_error(error: eyre::Error) -> impl IntoResponse {
    let error = format!("{error:?}");
    (
        StatusCode::INTERNAL_SERVER_ERROR, 
        Html(ansi_to_html::convert_escaped(&error).unwrap_or(error))
    )
}

pub fn map_std_error(error: impl std::error::Error) -> impl IntoResponse {
    let error = format!("{error:?}");
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}
