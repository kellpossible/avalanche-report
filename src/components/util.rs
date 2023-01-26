use html_builder::{Html5, Node};
use std::fmt::Write;

pub fn stylesheet(node: &mut Node, href: &str) {
    node.link()
        .attr(r#"rel="stylesheet""#)
        .attr(&format!(r#"href="{href}""#));
}

pub fn script_src(node: &mut Node, src: &str) {
    node.script().attr(&format!(r#"src="{src}""#));
}

pub fn script_inline(node: &mut Node, script: &str) -> eyre::Result<()> {
    node.script().write_str(script).map_err(eyre::Error::from)
}
