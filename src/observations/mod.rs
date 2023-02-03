use axum::{
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Form, Router,
};
use html_builder::{Html5, Node};
use serde::Deserialize;
use std::fmt::Write;

use crate::{components, i18n::I18nLoader};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/submit", post(submit_handler))
}

fn render_body(body: &mut Node, _loader: I18nLoader) -> eyre::Result<()> {
    body.h1()
        .attr(r#"class="text-3xl font-bold""#)
        .write_str("Observation Position")?;
    body.p().write_str(
        "Specify the position of your observation before proceeding to the Google Form.",
    )?;
    body.br();
    let mut form = body
        .form()
        .attr(r#"class="space-y-2""#)
        .attr(r#"method="post""#)
        .attr(r#"action="/observations/submit""#);
    form.label()
        .attr(r#"for="position""#)
        .attr(r#"class="text-gray-700 text-sm font-bold mb-2""#)
        .write_str("Position of Observation")?;
    form.input()
        .attr(r#"type="text""#)
        .attr(r#"id="position""#)
        .attr(r#"name="position""#)
        .attr(r#"class="shadow appearance-none border rounded py-2 px-3 text-gray-700 leading-tight focus:outline-none focus:shadow-outline""#)
        .attr(r#"value="""#)
        .attr("readonly");
    form.div()
        .attr(r#"id="map""#)
        .attr(r#"style="width: 600px; height: 400px;""#);
    form.input()
        .attr(r#"type="submit""#)
        .attr(r#"class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline""#)
        .attr(r#"value="Go to Google Form""#);
    Ok(())
}

async fn index_handler(Extension(loader): Extension<I18nLoader>) -> impl IntoResponse {
    components::Base::builder()
        .i18n(loader.clone())
        .body(
            &(move |body: &mut Node| {
                render_body(body, loader.clone())?;
                Ok(())
            }),
        )
        .head(&|head: &mut Node| {
            head.title().write_str("Observation Position")?;
            components::stylesheet(head, "/dist/leaflet.css");
            components::script_src(head, "/dist/leaflet.js");
            components::stylesheet(head, "/dist/Leaflet.GeotagPhoto.css");
            components::script_src(head, "/dist/Leaflet.GeotagPhoto.js");
            components::stylesheet(head, "/dist/L.Control.MapCenterCoord.css");
            components::script_src(head, "/dist/L.Control.MapCenterCoord.js");
            head.style().write_str(include_str!("./map.css"))?;
            Ok(())
        })
        .body_scripts(&|body: &mut Node| {
            let map = include_str!("./map.js");
            components::script_inline(body, &map)?;
            Ok(())
        })
        .build()
        .into_response()
}

#[derive(Deserialize)]
struct SubmitForm {
    position: String,
}

async fn submit_handler(Form(query): Form<SubmitForm>) -> impl IntoResponse {
    let position = query.position;
    Redirect::to(&format!("https://docs.google.com/forms/d/e/1FAIpQLSf2LbBvZ1IRHxNEf-X7nVVA9g3sTsd7eT5ZA9GsDCPeV6HIfA/viewform?usp=pp_url&entry.554457675={position}"))
}
