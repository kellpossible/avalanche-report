use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

pub fn map_eyre_error(error: eyre::Error) -> Response {
    tracing::error!("{error:?}");
    let error = format!("{error:?}");
    let mut html = ansi_to_html::convert_with_opts(&error, &ansi_to_html::Opts::default())
        .unwrap_or(error)
        .replace('\n', "<br>");
    html.insert_str(0, "<pre>");
    html.push_str("</pre>");
    (StatusCode::INTERNAL_SERVER_ERROR, Html(html)).into_response()
}

pub fn map_std_error(error: impl std::error::Error) -> Response {
    let error = format!("{error:?}");
    (StatusCode::INTERNAL_SERVER_ERROR, error).into_response()
}
