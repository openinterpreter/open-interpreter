#![warn(argument_comment_mismatch)]
#![warn(uncommented_anonymous_literal_argument)]

fn run_git_for_stdout(repo_root: &str, args: Vec<&str>, env: Option<&str>) -> String {
    let _ = (repo_root, args, env);
    String::new()
}

// Keep the call split across lines so this fixture tests multiline arguments.
#[rustfmt::skip]
fn main() {
    let _ = run_git_for_stdout(
        "/tmp/repo",
        vec!["rev-parse", "HEAD"],
        /*env*/ None,
    );
}
