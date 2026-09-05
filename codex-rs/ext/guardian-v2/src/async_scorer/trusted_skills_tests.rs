use super::GuardianTrustedSkillsFragment;
use super::MAX_TRUSTED_SKILL_PATHS_BYTES;
use super::MAX_TRUSTED_SKILL_TOKENS;
use super::MAX_TRUSTED_SKILLS;
use super::TRUSTED_SKILLS_PREFIX;
use super::TrustedSkillInvocations;
use super::TrustedSkillRoots;
use anyhow::Result;
use codex_extension_api::ContextualUserFragment;
use codex_protocol::protocol::TruncationPolicy;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;

fn rendered_paths(paths: Vec<String>) -> Vec<String> {
    let rendered = GuardianTrustedSkillsFragment { paths }.render();
    assert!(rendered.len() <= TruncationPolicy::Tokens(MAX_TRUSTED_SKILL_TOKENS).byte_budget());
    serde_json::from_str(
        rendered
            .strip_prefix(TRUSTED_SKILLS_PREFIX)
            .expect("trusted skill context should contain JSON evidence"),
    )
    .expect("trusted skill evidence should remain valid JSON")
}

#[test]
fn renders_verified_skill_paths() {
    assert_eq!(
        rendered_paths(vec!["/home/user/.codex/skills/demo/SKILL.md".to_owned()]),
        vec!["/home/user/.codex/skills/demo/SKILL.md"],
    );
}

#[test]
fn bounds_escaped_skill_paths_without_corrupting_json_or_utf8() {
    let paths = (0..MAX_TRUSTED_SKILLS)
        .map(|index| {
            format!(
                "/home/user/.codex/skills/{index:03}/{}SKILL.md",
                "\u{0001}é".repeat(80)
            )
        })
        .collect::<Vec<_>>();
    let retained = rendered_paths(paths.clone());

    assert!(!retained.is_empty());
    assert!(retained.len() < paths.len());
    assert!(retained.iter().all(|path| paths.contains(path)));
}

#[test]
fn preserves_multiple_invoked_skill_paths() {
    assert_eq!(
        rendered_paths(vec![
            "/home/user/.codex/skills/first/SKILL.md".to_owned(),
            "/home/user/.codex/skills/second/SKILL.md".to_owned(),
        ]),
        vec![
            "/home/user/.codex/skills/first/SKILL.md",
            "/home/user/.codex/skills/second/SKILL.md",
        ],
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trusts_only_user_owned_skill_roots() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let test = test_codex().build_with_auto_env(&server).await?;
    let codex_skill = test.home.path().join("skills/trusted/SKILL.md");
    let agents_skill = test
        .home
        .path()
        .join("user/.agents/skills/also-trusted/SKILL.md");
    let repo_skill = test
        .home
        .path()
        .join("workspace/.agents/skills/untrusted/SKILL.md");
    for path in [&codex_skill, &agents_skill, &repo_skill] {
        std::fs::create_dir_all(path.parent().expect("skill parent"))?;
        std::fs::write(path, "trusted skill instructions")?;
    }

    let roots = TrustedSkillRoots {
        roots: vec![
            test.home.path().join("skills"),
            test.home.path().join("user/.agents/skills"),
        ],
    };
    for trusted_path in [&codex_skill, &agents_skill] {
        assert_eq!(
            roots.trusted_skill_path(trusted_path.to_str().expect("UTF-8 skill path")),
            Some(trusted_path.canonicalize()?.display().to_string()),
        );
    }
    for untrusted_path in [
        repo_skill.to_str().expect("UTF-8 skill path"),
        "skill://executor/example/SKILL.md",
    ] {
        assert_eq!(roots.trusted_skill_path(untrusted_path), None);
    }

    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_skills_that_escape_user_roots_through_symlinks() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let test = test_codex().build_with_auto_env(&server).await?;
    let trusted_root = test.home.path().join("skills");
    let outside_root = test.home.path().join("outside");
    std::fs::create_dir_all(&trusted_root)?;
    std::fs::create_dir_all(&outside_root)?;
    let outside_skill = outside_root.join("SKILL.md");
    std::fs::write(&outside_skill, "untrusted external skill")?;
    let linked_skill = trusted_root.join("linked-skill");
    std::os::unix::fs::symlink(&outside_root, &linked_skill)?;

    let roots = TrustedSkillRoots::from_config(&test.config);
    assert_eq!(
        roots.trusted_skill_path(
            linked_skill
                .join("SKILL.md")
                .to_str()
                .expect("UTF-8 skill path"),
        ),
        None,
    );

    Ok(())
}

#[test]
fn invoked_skill_paths_are_deduplicated_and_bounded() {
    let mut skills = TrustedSkillInvocations::default();
    for index in 0..MAX_TRUSTED_SKILLS.saturating_mul(2) {
        let path = format!("/home/user/.codex/skills/{index:03}/SKILL.md");
        skills.record(path.clone());
        skills.record(path);
    }

    assert_eq!(
        skills.into_paths(),
        (0..MAX_TRUSTED_SKILLS)
            .map(|index| format!("/home/user/.codex/skills/{index:03}/SKILL.md"))
            .collect::<Vec<_>>()
    );

    let mut bounded = TrustedSkillInvocations::default();
    for index in 0..MAX_TRUSTED_SKILLS {
        bounded.record(format!("{index:03}{}", "x".repeat(500)));
    }
    let snapshot = bounded.into_paths();
    assert!(snapshot.len() < MAX_TRUSTED_SKILLS);
    assert!(snapshot.iter().map(String::len).sum::<usize>() <= MAX_TRUSTED_SKILL_PATHS_BYTES);
}
