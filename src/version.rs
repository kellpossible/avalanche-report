pub async fn handler() -> &'static str {
    git_version::git_version!()
}
