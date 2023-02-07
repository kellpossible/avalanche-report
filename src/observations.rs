use axum::{
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;

use crate::{templates, state::AppState};

pub fn router() -> Router<AppState>
{
    Router::new()
        .route("/", get(templates::create_handler("observations.html")))
        .route("/submit", post(submit_handler))
}

#[derive(Deserialize)]
struct SubmitForm {
    position: String,
}

async fn submit_handler(Form(query): Form<SubmitForm>) -> impl IntoResponse {
    let position = query.position;
    Redirect::to(&format!("https://docs.google.com/forms/d/e/1FAIpQLSf2LbBvZ1IRHxNEf-X7nVVA9g3sTsd7eT5ZA9GsDCPeV6HIfA/viewform?usp=pp_url&entry.554457675={position}"))
}
