use axum::{
    http::StatusCode,
    response::{ErrorResponse, Html},
};

pub fn map_eyre_error(error: eyre::Error) -> ErrorResponse {
    let error = format!("{error:?}");
    ErrorResponse::from((
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(
            ansi_to_html::convert_escaped(&error)
                .unwrap_or(error)
                .replace('\n', "<br>"),
        ),
    ))
}

pub fn map_std_error(error: impl std::error::Error) -> ErrorResponse {
    let error = format!("{error:?}");
    ErrorResponse::from((StatusCode::INTERNAL_SERVER_ERROR, error))
}
