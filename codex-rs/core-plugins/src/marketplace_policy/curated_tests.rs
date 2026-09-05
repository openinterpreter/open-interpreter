//! Checks the shared curated Git identity while preserving other managed marketplace exemptions.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn curated_catalogs_apply_the_existing_git_source_policy() {
    let home = TempDir::new().unwrap();
    for (rule, allowed) in [
        ("", false),
        (
            "source = 'git'\nurl = 'https://github.com/openai/plugins.git'",
            true,
        ),
        (
            "source = 'git'\nurl = 'https://github.com/openai/plugins'",
            true,
        ),
        ("source = 'git'\nurl = 'openai/plugins'", true),
        (
            "source = 'git'\nurl = 'https://github.com/other/plugins.git'",
            false,
        ),
        (
            "source = 'host_pattern'\nhost_pattern = '^github[.]com$'",
            true,
        ),
        (
            "source = 'host_pattern'\nhost_pattern = '^git[.]example[.]com$'",
            false,
        ),
        (
            "source = 'git'\nurl = 'https://github.com/openai/plugins.git'\nref = 'main'",
            false,
        ),
        (
            "source = 'git'\nurl = 'openai/plugins'\n[marketplaces.allowed_sources.invalid]\nsource = 'host_pattern'\nhost_pattern = '('",
            false,
        ),
    ] {
        let rule = if rule.is_empty() {
            String::new()
        } else {
            format!("[marketplaces.allowed_sources.curated]\n{rule}")
        };
        let stack = config_layer_stack(&format!(
            "[marketplaces]\nrestrict_to_allowed_sources = true\n{rule}"
        ));
        let policy = MarketplacePolicy::from_requirements(stack.requirements());
        for (name, manifest) in [
            (
                OPENAI_CURATED_MARKETPLACE_NAME,
                curated_plugins_repo_path(home.path()).join(".agents/plugins/marketplace.json"),
            ),
            (
                OPENAI_API_CURATED_MARKETPLACE_NAME,
                curated_plugins_api_marketplace_path(home.path()),
            ),
        ] {
            let manifest = AbsolutePathBuf::try_from(manifest).unwrap();
            assert_eq!(
                policy
                    .validate_install(&stack, home.path(), &manifest, name)
                    .is_ok(),
                allowed
            );
        }
    }
}

#[test]
fn a_local_allowlist_cannot_authorize_curated_but_other_managed_sources_remain_allowed() {
    let home = TempDir::new().unwrap();
    let root = curated_plugins_repo_path(home.path());
    let stack = config_layer_stack(&format!(
        "[marketplaces]\nrestrict_to_allowed_sources = true\n[marketplaces.allowed_sources.local]\nsource = 'local'\npath = {root:?}"
    ));
    let policy = MarketplacePolicy::from_requirements(stack.requirements());
    assert!(
        policy
            .validate_git_source(OPENAI_PLUGINS_GIT_URL, /*ref_name*/ None)
            .is_err()
    );
    // Windows test sandboxes may omit USERPROFILE, leaving no primary runtime cache root.
    let primary_runtime = primary_runtime_marketplace_root()
        .map(|root| (OPENAI_PRIMARY_RUNTIME_MARKETPLACE_NAME, root));
    for (name, root) in [
        (
            OPENAI_BUNDLED_MARKETPLACE_NAME,
            home.path().join(".tmp/bundled-marketplaces/openai-bundled"),
        ),
        (
            OPENAI_BUNDLED_ALPHA_MARKETPLACE_NAME,
            home.path()
                .join(".tmp/bundled-marketplaces/openai-bundled-alpha"),
        ),
    ]
    .into_iter()
    .chain(primary_runtime)
    {
        let manifest =
            AbsolutePathBuf::try_from(root.join(".agents/plugins/marketplace.json")).unwrap();
        assert_eq!(
            policy.validate_install(&stack, home.path(), &manifest, name),
            Ok(())
        );
    }
}
