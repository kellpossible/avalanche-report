mod base;
mod util;
pub use base::Base;
use html_builder::Node;
pub use util::*;

pub trait ComponentNode {
    fn run(&self, node: &mut Node) -> eyre::Result<()>;
}

impl<F> ComponentNode for F
where
    F: Fn(&mut Node) -> eyre::Result<()>,
{
    fn run(&self, node: &mut Node) -> eyre::Result<()> {
        self(node)
    }
}
