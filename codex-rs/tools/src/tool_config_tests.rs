use codex_features::Feature;
use codex_features::Features;
use codex_protocol::config_types::ModeKind;
use pretty_assertions::assert_eq;

use super::*;

fn unified_exec_features() -> Features {
    let mut features = Features::with_defaults();
    features.enable(Feature::ShellTool);
    features.enable(Feature::ShellZshFork);
    features.enable(Feature::UnifiedExec);
    features.enable(Feature::UnifiedExecZshFork);
    features
}

#[test]
fn request_user_input_modes_follow_default_mode_feature() {
    let mut features = Features::with_defaults();
    features.disable(Feature::DefaultModeRequestUserInput);
    assert_eq!(
        request_user_input_available_modes(&features),
        vec![ModeKind::Plan]
    );

    features.enable(Feature::DefaultModeRequestUserInput);
    assert_eq!(
        request_user_input_available_modes(&features),
        vec![ModeKind::Default, ModeKind::Plan]
    );
}

#[test]
fn unified_exec_shell_mode_uses_zsh_fork_only_when_all_inputs_match() {
    let exe = std::env::current_exe().expect("current exe path");
    let shell = exe.clone();
    let features = unified_exec_features();

    let mode = UnifiedExecShellMode::for_session(
        &features,
        ToolUserShellType::Zsh,
        Some(&shell),
        Some(&exe),
    );
    if cfg!(unix) {
        assert!(matches!(mode, UnifiedExecShellMode::ZshFork(_)));
    } else {
        assert_eq!(mode, UnifiedExecShellMode::Direct);
    }

    for feature in [
        Feature::ShellTool,
        Feature::UnifiedExec,
        Feature::ShellZshFork,
        Feature::UnifiedExecZshFork,
    ] {
        let mut missing_dependency = features.clone();
        missing_dependency.disable(feature);
        assert_eq!(
            UnifiedExecShellMode::for_session(
                &missing_dependency,
                ToolUserShellType::Zsh,
                Some(&shell),
                Some(&exe),
            ),
            UnifiedExecShellMode::Direct,
            "disabling {feature:?} must disable zsh-fork composition"
        );
    }

    for shell_type in [
        ToolUserShellType::Bash,
        ToolUserShellType::PowerShell,
        ToolUserShellType::Sh,
        ToolUserShellType::Cmd,
    ] {
        assert_eq!(
            UnifiedExecShellMode::for_session(&features, shell_type, Some(&shell), Some(&exe),),
            UnifiedExecShellMode::Direct
        );
    }

    for (shell_path, wrapper_path) in [(None, Some(&exe)), (Some(&shell), None)] {
        assert_eq!(
            UnifiedExecShellMode::for_session(
                &features,
                ToolUserShellType::Zsh,
                shell_path,
                wrapper_path,
            ),
            UnifiedExecShellMode::Direct
        );
    }
}
