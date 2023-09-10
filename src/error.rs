use axum::{
    http::StatusCode,
    response::{ErrorResponse, Html},
};

pub fn map_eyre_error(error: eyre::Error) -> ErrorResponse {
    tracing::error!("{error:?}");
    let error = format!("{error:?}");
    let mut html = ansi_to_html::convert_escaped(&error)
        .unwrap_or(error)
        .replace('\n', "<br>");
    html.insert_str(0, "<pre>");
    html.push_str("</pre>");
    ErrorResponse::from((StatusCode::INTERNAL_SERVER_ERROR, Html(html)))
}

pub fn map_std_error(error: impl std::error::Error) -> ErrorResponse {
    let error = format!("{error:?}");
    ErrorResponse::from((StatusCode::INTERNAL_SERVER_ERROR, error))
}
