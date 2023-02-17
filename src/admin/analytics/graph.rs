use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension,
};

use crate::{
    state::AppState,
    templates::{render, TemplatesWithContext},
};

pub async fn handler(
    State(state): State<AppState>,
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<Response> {
    render(&templates.environment, "admin/analytics/graph.html", &())
}
