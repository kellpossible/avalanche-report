/// Workaround for https://github.com/rust-lang/rust/issues/100013
pub fn assert_send_stream<R>(
    s: impl futures::Stream<Item = R> + Send,
) -> impl futures::Stream<Item = R> + Send {
    s
}
