use super::effective_file_system_sandbox_policy;
use super::intersect_permission_profiles;
use super::intersect_permission_profiles_with_context;
use super::materialize_additional_permissions;
use super::materialize_additional_permissions_with_context;
use super::merge_file_system_policy_with_additional_permissions;
use super::normalize_additional_permissions;
use super::normalize_additional_permissions_with_context;
use super::should_require_platform_sandbox;
use codex_protocol::models::AdditionalPermissionProfile as PermissionProfile;
use codex_protocol::models::FileSystemPermissions;
use codex_protocol::models::NetworkPermissions;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::FileSystemSandboxPolicyContext;
use codex_protocol::permissions::FileSystemSpecialPath;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::LegacyAppPathString;
use codex_utils_path_uri::PathConvention;
use codex_utils_path_uri::PathUri;
use dunce::canonicalize;
use pretty_assertions::assert_eq;
#[cfg(unix)]
use std::path::Path;
use tempfile::TempDir;

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[test]
fn full_access_restricted_policy_skips_platform_sandbox_when_network_is_enabled() {
    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Write,
        missing_path_behavior: None,
    }]);

    assert_eq!(
        should_require_platform_sandbox(
            &policy,
            NetworkSandboxPolicy::Enabled,
            /*has_managed_network_requirements*/ false
        ),
        false
    );
}

#[test]
fn root_write_policy_with_carveouts_still_uses_platform_sandbox() {
    let blocked = AbsolutePathBuf::resolve_path_against_base(
        "blocked",
        std::env::current_dir().expect("current dir"),
    );
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
            missing_path_behavior: None,
        },
        FileSystemSandboxEntry {
            path: blocked.into(),
            access: FileSystemAccessMode::Deny,
            missing_path_behavior: None,
        },
    ]);

    assert_eq!(
        should_require_platform_sandbox(
            &policy,
            NetworkSandboxPolicy::Enabled,
            /*has_managed_network_requirements*/ false
        ),
        true
    );
}

#[test]
fn full_access_restricted_policy_still_uses_platform_sandbox_for_restricted_network() {
    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Write,
        missing_path_behavior: None,
    }]);

    assert_eq!(
        should_require_platform_sandbox(
            &policy,
            NetworkSandboxPolicy::Restricted,
            /*has_managed_network_requirements*/ false
        ),
        true
    );
}

#[test]
fn normalize_additional_permissions_preserves_network() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let permissions = normalize_additional_permissions(PermissionProfile {
        network: Some(NetworkPermissions {
            enabled: Some(true),
        }),
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![path.clone()]),
            Some(vec![path.clone()]),
        )),
    })
    .expect("permissions");

    assert_eq!(
        permissions.network,
        Some(NetworkPermissions {
            enabled: Some(true),
        })
    );
    assert_eq!(
        permissions.file_system,
        Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![path.clone()]),
            Some(vec![path]),
        ))
    );
}

#[test]
fn normalize_additional_permissions_only_checks_convention_with_context() {
    let windows_path = PathUri::parse("file:///C:/workspace/out").expect("Windows path URI");
    let permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry::new(
                FileSystemPath::Path { path: windows_path },
                FileSystemAccessMode::Write,
            )],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        normalize_additional_permissions(permissions.clone()).expect("context-free permissions"),
        permissions
    );

    let cwd = PathUri::parse("file:///workspace").expect("POSIX cwd URI");
    let workspace_roots = [cwd.clone()];
    let context = FileSystemSandboxPolicyContext {
        cwd: &cwd,
        workspace_roots: &workspace_roots,
        user_home_dir: None,
        temporary_directories: None,
    };
    assert!(
        normalize_additional_permissions_with_context(permissions.clone(), &context).is_err(),
        "context-aware normalization should reject mismatched conventions"
    );

    let cwd = LegacyAppPathString::from_string(
        r"\\?\Volume{00000000-0000-0000-0000-000000000000}\workspace",
    )
    .to_path_uri(PathConvention::Windows)
    .expect("opaque Windows cwd");
    assert!(cwd.is_opaque(), "volume GUID cwd should stay opaque");
    let workspace_roots = [cwd.clone()];
    let context = FileSystemSandboxPolicyContext {
        cwd: &cwd,
        workspace_roots: &workspace_roots,
        user_home_dir: None,
        temporary_directories: None,
    };

    assert_eq!(
        normalize_additional_permissions_with_context(permissions.clone(), &context)
            .expect("absolute path with opaque cwd"),
        permissions
    );
}

#[cfg(unix)]
#[test]
fn normalize_additional_permissions_preserves_symlinked_write_paths() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let real_root = temp_dir.path().join("real");
    let link_root = temp_dir.path().join("link");
    let write_dir = real_root.join("write");
    std::fs::create_dir_all(&write_dir).expect("create write dir");
    symlink_dir(&real_root, &link_root).expect("create symlinked root");

    let link_write_dir =
        AbsolutePathBuf::from_absolute_path(link_root.join("write")).expect("link write dir");
    let permissions = normalize_additional_permissions(PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![]),
            Some(vec![link_write_dir]),
        )),
        ..Default::default()
    })
    .expect("permissions");

    assert_eq!(
        permissions.file_system,
        Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![]),
            Some(vec![
                AbsolutePathBuf::from_absolute_path(link_root.join("write"))
                    .expect("link write dir"),
            ]),
        ))
    );
}

#[test]
fn normalize_additional_permissions_rejects_glob_read_grants() {
    let err = normalize_additional_permissions(PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry {
                path: FileSystemPath::GlobPattern {
                    pattern: "**/*.env".to_string(),
                },
                access: FileSystemAccessMode::Read,
                missing_path_behavior: None,
            }],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    })
    .expect_err("read glob permissions are unsupported");

    assert_eq!(
        err,
        "glob file system permissions only support deny-read entries"
    );
}

#[test]
fn normalize_additional_permissions_preserves_deny_globs() {
    let permissions = normalize_additional_permissions(PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry {
                path: FileSystemPath::GlobPattern {
                    pattern: "**/*.env".to_string(),
                },
                access: FileSystemAccessMode::Deny,
                missing_path_behavior: None,
            }],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        }),
        ..Default::default()
    })
    .expect("deny glob permissions are supported");

    assert_eq!(
        permissions,
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![FileSystemSandboxEntry {
                    path: FileSystemPath::GlobPattern {
                        pattern: "**/*.env".to_string(),
                    },
                    access: FileSystemAccessMode::Deny,
                    missing_path_behavior: None,
                }],
                glob_scan_max_depth: std::num::NonZeroUsize::new(2),
            }),
            ..Default::default()
        }
    );
}

#[test]
fn normalize_additional_permissions_drops_empty_nested_profiles() {
    let permissions = normalize_additional_permissions(PermissionProfile {
        network: Some(NetworkPermissions { enabled: None }),
        file_system: Some(FileSystemPermissions::default()),
    })
    .expect("permissions");

    assert_eq!(permissions, PermissionProfile::default());
}

#[test]
fn materialize_additional_permissions_preserves_authority_and_constraints() {
    use FileSystemAccessMode::Deny;
    use FileSystemAccessMode::Read;
    use FileSystemAccessMode::Write;

    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let home = PathUri::from_host_native_path("~").expect("host home");
    let project_path = |subpath: &str| FileSystemPath::Special {
        value: FileSystemSpecialPath::project_roots(Some(subpath.to_owned())),
    };
    let deny_glob = |pattern: String| {
        FileSystemSandboxEntry::new(FileSystemPath::GlobPattern { pattern }, Deny)
    };
    let profile = |entries| PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries,
            glob_scan_max_depth: std::num::NonZeroUsize::new(/*n*/ 3),
        }),
        ..Default::default()
    };
    let reopened = FileSystemSandboxEntry::new(cwd.join("private/reopened").into(), Write);
    let requested = profile(vec![
        FileSystemSandboxEntry::new(project_path("."), Write),
        FileSystemSandboxEntry::new(project_path("private"), Deny),
        FileSystemSandboxEntry::skip_missing_path(project_path("readonly"), Read),
        FileSystemSandboxEntry::new(project_path("private/reopened"), Write),
        reopened.clone(),
        deny_glob("**/*.env".to_owned()),
        deny_glob("~".to_owned()),
        deny_glob("~/private/*.env".to_owned()),
    ]);
    let expected = profile(vec![
        FileSystemSandboxEntry::new(cwd.clone().into(), Write),
        FileSystemSandboxEntry::new(cwd.join("private").into(), Deny),
        FileSystemSandboxEntry::skip_missing_path(cwd.join("readonly").into(), Read),
        reopened,
        deny_glob(cwd.join("**/*.env").to_string_lossy().into_owned()),
        deny_glob(home.inferred_native_path_string()),
        deny_glob(
            home.join("private/*.env")
                .expect("home-relative deny glob")
                .inferred_native_path_string(),
        ),
    ]);

    assert_eq!(
        materialize_additional_permissions(requested, cwd.as_path())
            .expect("materialized permissions"),
        expected
    );
}

#[test]
fn materialize_additional_permissions_ignores_empty_tmpdir_deny() {
    let cwd = PathUri::parse("file:///workspace").expect("cwd URI");
    let workspace_roots = [cwd.clone()];
    let temporary_directories = [];
    let context = FileSystemSandboxPolicyContext {
        cwd: &cwd,
        workspace_roots: &workspace_roots,
        user_home_dir: None,
        temporary_directories: Some(&temporary_directories),
    };
    let write = FileSystemSandboxEntry::new(
        FileSystemPath::Path { path: cwd.clone() },
        FileSystemAccessMode::Write,
    );
    let permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![
                write.clone(),
                FileSystemSandboxEntry::new(
                    FileSystemPath::Special {
                        value: FileSystemSpecialPath::Tmpdir,
                    },
                    FileSystemAccessMode::Deny,
                ),
            ],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        materialize_additional_permissions_with_context(permissions, &context)
            .expect("permissions"),
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![write],
                glob_scan_max_depth: None,
            }),
            ..Default::default()
        }
    );
}

#[test]
fn intersect_permission_profiles_preserves_explicit_empty_requested_reads() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![]),
            Some(vec![path]),
        )),
        ..Default::default()
    };
    let granted = requested.clone();

    assert_eq!(
        intersect_permission_profiles(requested.clone(), granted, temp_dir.path()),
        requested
    );
}

#[test]
fn intersect_permission_profiles_drops_ungranted_nonempty_path_requests() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![path]),
            /*write*/ None,
        )),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, PermissionProfile::default(), temp_dir.path()),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_drops_explicit_empty_reads_without_grant() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![]),
            Some(vec![path]),
        )),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, PermissionProfile::default(), temp_dir.path()),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_preserves_parent_relative_project_root_restriction() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = AbsolutePathBuf::from_absolute_path(temp_dir.path()).expect("absolute temp dir");
    let cwd = root.join("project");
    let outside_child = root.join("outside/child");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![
                FileSystemSandboxEntry::new(
                    FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                    FileSystemAccessMode::Write,
                ),
                FileSystemSandboxEntry::new(
                    FileSystemPath::Special {
                        value: FileSystemSpecialPath::project_roots(Some("../outside".to_string())),
                    },
                    FileSystemAccessMode::Read,
                ),
            ],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            /*read*/ None,
            Some(vec![outside_child]),
        )),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted, cwd.as_path()),
        PermissionProfile::default()
    );
}

#[cfg(windows)]
#[test]
fn intersect_permission_profiles_rejects_symbolic_slash_tmp_grants() {
    let cwd = TempDir::new().expect("tempdir");
    let slash_tmp = AbsolutePathBuf::from_absolute_path("/tmp").expect("absolute tmp path");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            /*read*/ None,
            Some(vec![slash_tmp]),
        )),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry::new(
                FileSystemPath::Special {
                    value: FileSystemSpecialPath::SlashTmp,
                },
                FileSystemAccessMode::Write,
            )],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted.clone(), cwd.path()),
        PermissionProfile::default()
    );
    assert_eq!(
        intersect_permission_profiles(granted.clone(), granted, cwd.path()),
        PermissionProfile::default()
    );
}

#[cfg(windows)]
#[test]
fn intersect_permission_profiles_preserves_deny_across_case_variant_grant() {
    use FileSystemAccessMode::Deny;
    use FileSystemAccessMode::Write;

    let requested_root =
        AbsolutePathBuf::from_absolute_path(r"C:\workspace").expect("absolute requested root");
    let granted_root =
        AbsolutePathBuf::from_absolute_path(r"C:\WORKSPACE").expect("absolute granted root");
    let requested_write = FileSystemSandboxEntry::new(requested_root.clone().into(), Write);
    let requested_deny = FileSystemSandboxEntry::new(requested_root.join("secret").into(), Deny);
    let granted_write = FileSystemSandboxEntry::new(granted_root.into(), Write);
    let profile = |entries| PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries,
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(
            profile(vec![requested_write, requested_deny.clone()]),
            profile(vec![granted_write.clone()]),
            requested_root.as_path(),
        ),
        profile(vec![granted_write, requested_deny])
    );
}

#[test]
fn intersect_permission_profiles_preserves_denies_for_unresolved_tmpdir() {
    let cwd = PathUri::parse("file:///workspace").expect("cwd URI");
    let context = FileSystemSandboxPolicyContext {
        cwd: &cwd,
        workspace_roots: std::slice::from_ref(&cwd),
        user_home_dir: None,
        temporary_directories: None,
    };
    let write = FileSystemSandboxEntry::new(
        FileSystemPath::Special {
            value: FileSystemSpecialPath::Tmpdir,
        },
        FileSystemAccessMode::Write,
    );
    let deny = FileSystemSandboxEntry::new(
        PathUri::parse("file:///tmp/private").expect("deny").into(),
        FileSystemAccessMode::Deny,
    );
    let profile = |entries| PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries,
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };
    let grant = profile(vec![write.clone()]);
    let grant_with_deny = profile(vec![write, deny]);

    for (requested, granted) in [
        (grant_with_deny.clone(), grant.clone()),
        (grant, grant_with_deny.clone()),
        (grant_with_deny.clone(), grant_with_deny.clone()),
    ] {
        assert_eq!(
            intersect_permission_profiles_with_context(requested, granted, &context),
            grant_with_deny
        );
    }
}

#[test]
fn intersect_permission_profiles_preserves_rooted_first_segment_glob_deny() {
    use FileSystemAccessMode::Deny;
    use FileSystemAccessMode::Write;

    for (cwd_uri, grant_uri, pattern) in [
        ("file:///workspace", "file:///fooX", "/foo*/*"),
        ("file:///workspace", "file:///foo%5Cbar", r"/foo\\*/*"),
        ("file:///C:/workspace", "file:///C:/fooX", r"C:\foo*\*"),
        (
            "file://server/share/workspace",
            "file://server/share/fooX",
            r"\\server\share\foo*\*",
        ),
    ] {
        let cwd = PathUri::parse(cwd_uri).expect("cwd URI");
        let grant_path = PathUri::parse(grant_uri).expect("grant URI");
        let workspace_roots = [cwd.clone()];
        let context = FileSystemSandboxPolicyContext {
            cwd: &cwd,
            workspace_roots: &workspace_roots,
            user_home_dir: None,
            temporary_directories: None,
        };
        let grant_entry =
            FileSystemSandboxEntry::new(FileSystemPath::Path { path: grant_path }, Write);
        let deny_entry = FileSystemSandboxEntry::new(
            FileSystemPath::GlobPattern {
                pattern: pattern.to_string(),
            },
            Deny,
        );
        let profile = |entries| PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries,
                glob_scan_max_depth: None,
            }),
            ..Default::default()
        };

        assert_eq!(
            intersect_permission_profiles_with_context(
                profile(vec![grant_entry.clone(), deny_entry.clone()]),
                profile(vec![grant_entry.clone()]),
                &context,
            ),
            profile(vec![grant_entry, deny_entry]),
            "deny glob should survive for {pattern}",
        );
    }
}

#[test]
fn intersect_permission_profiles_preserves_opaque_child_deny() {
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    use FileSystemAccessMode::Deny;
    use FileSystemAccessMode::Write;

    let temp_dir = TempDir::new().expect("create temp dir");
    let root = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    #[cfg(unix)]
    let opaque_name = OsString::from_vec(b"private-\xff".to_vec());
    #[cfg(windows)]
    let opaque_name = OsString::from_wide(&[
        u16::from(b'p'),
        u16::from(b'r'),
        u16::from(b'i'),
        u16::from(b'v'),
        u16::from(b'a'),
        u16::from(b't'),
        u16::from(b'e'),
        u16::from(b'-'),
        0xD800,
    ]);
    let deny_path = root.join(opaque_name);
    let requested_write = FileSystemSandboxEntry::new(root.clone().into(), Write);
    let requested_deny = FileSystemSandboxEntry::new(deny_path.into(), Deny);
    let granted_write = FileSystemSandboxEntry::new(root.clone().into(), Write);
    let profile = |entries| PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries,
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(
            profile(vec![requested_write, requested_deny.clone()]),
            profile(vec![granted_write.clone()]),
            root.as_path(),
        ),
        profile(vec![granted_write, requested_deny])
    );
}

#[test]
fn intersect_permission_profiles_accepts_child_path_granted_for_requested_cwd() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let child = cwd.join("child");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                },
                access: FileSystemAccessMode::Write,
                missing_path_behavior: None,
            }],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            /*read*/ None,
            Some(vec![child]),
        )),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted.clone(), cwd.as_path()),
        granted
    );
}

#[test]
fn intersect_permission_profiles_materializes_cwd_grant_for_reuse() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let request_cwd = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("request-cwd"))
        .expect("absolute request cwd");
    let later_cwd = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("later-cwd"))
        .expect("absolute later cwd");
    let cwd_write_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                },
                access: FileSystemAccessMode::Write,
                missing_path_behavior: None,
            }],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    let intersected = intersect_permission_profiles(
        cwd_write_permissions.clone(),
        cwd_write_permissions,
        request_cwd.as_path(),
    );

    assert_eq!(
        intersected,
        PermissionProfile {
            file_system: Some(FileSystemPermissions::from_read_write_roots(
                /*read*/ None,
                Some(vec![request_cwd]),
            )),
            ..Default::default()
        }
    );
    assert_eq!(
        intersect_permission_profiles(
            PermissionProfile {
                file_system: Some(FileSystemPermissions::from_read_write_roots(
                    /*read*/ None,
                    Some(vec![later_cwd.join("child")]),
                )),
                ..Default::default()
            },
            intersected,
            later_cwd.as_path(),
        ),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_deduplicates_materialized_grants() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd =
        AbsolutePathBuf::from_absolute_path(temp_dir.path().join("cwd")).expect("absolute cwd");
    let permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![
                FileSystemSandboxEntry {
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                    },
                    access: FileSystemAccessMode::Write,
                    missing_path_behavior: None,
                },
                FileSystemSandboxEntry {
                    path: cwd.clone().into(),
                    access: FileSystemAccessMode::Write,
                    missing_path_behavior: None,
                },
            ],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(permissions.clone(), permissions, cwd.as_path()),
        PermissionProfile {
            file_system: Some(FileSystemPermissions::from_read_write_roots(
                /*read*/ None,
                Some(vec![cwd]),
            )),
            ..Default::default()
        }
    );
}

#[test]
fn intersect_permission_profiles_materializes_cwd_deny_entries() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let request_cwd = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("request-cwd"))
        .expect("absolute request cwd");
    let permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![
                FileSystemSandboxEntry {
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                    access: FileSystemAccessMode::Write,
                    missing_path_behavior: None,
                },
                FileSystemSandboxEntry {
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                    },
                    access: FileSystemAccessMode::Deny,
                    missing_path_behavior: None,
                },
            ],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(permissions.clone(), permissions, request_cwd.as_path()),
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![
                    FileSystemSandboxEntry {
                        path: FileSystemPath::Special {
                            value: FileSystemSpecialPath::Root,
                        },
                        access: FileSystemAccessMode::Write,
                        missing_path_behavior: None,
                    },
                    FileSystemSandboxEntry {
                        path: request_cwd.into(),
                        access: FileSystemAccessMode::Deny,
                        missing_path_behavior: None,
                    },
                ],
                glob_scan_max_depth: None,
            }),
            ..Default::default()
        }
    );
}

#[test]
fn intersect_permission_profiles_drops_deny_entries_without_filesystem_grants() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let secret = cwd.join("secret");
    let requested = PermissionProfile {
        network: Some(NetworkPermissions {
            enabled: Some(true),
        }),
        file_system: Some(FileSystemPermissions {
            entries: vec![
                FileSystemSandboxEntry {
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                    },
                    access: FileSystemAccessMode::Write,
                    missing_path_behavior: None,
                },
                FileSystemSandboxEntry {
                    path: secret.into(),
                    access: FileSystemAccessMode::Deny,
                    missing_path_behavior: None,
                },
            ],
            glob_scan_max_depth: None,
        }),
    };
    let granted = PermissionProfile {
        network: Some(NetworkPermissions {
            enabled: Some(true),
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted.clone(), cwd.as_path()),
        granted
    );
}

#[test]
fn intersect_permission_profiles_rejects_concrete_grants_matched_by_requested_deny_globs() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let env_file = cwd.join(".env");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![
                FileSystemSandboxEntry {
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                    },
                    access: FileSystemAccessMode::Write,
                    missing_path_behavior: None,
                },
                FileSystemSandboxEntry {
                    path: FileSystemPath::GlobPattern {
                        pattern: "**/*.env".to_string(),
                    },
                    access: FileSystemAccessMode::Deny,
                    missing_path_behavior: None,
                },
            ],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        }),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            /*read*/ None,
            Some(vec![env_file]),
        )),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted, cwd.as_path()),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_materializes_relative_deny_globs_for_reuse() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let request_cwd = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("request-cwd"))
        .expect("absolute request cwd");
    let later_cwd = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("later-cwd"))
        .expect("absolute later cwd");
    let cwd_write = FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
        },
        access: FileSystemAccessMode::Write,
        missing_path_behavior: None,
    };
    let deny_env_files = FileSystemSandboxEntry {
        path: FileSystemPath::GlobPattern {
            pattern: "**/*.env".to_string(),
        },
        access: FileSystemAccessMode::Deny,
        missing_path_behavior: None,
    };
    let permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![cwd_write, deny_env_files],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        }),
        ..Default::default()
    };

    let intersected =
        intersect_permission_profiles(permissions.clone(), permissions, request_cwd.as_path());

    assert_eq!(
        intersected,
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![
                    FileSystemSandboxEntry {
                        path: request_cwd.clone().into(),
                        access: FileSystemAccessMode::Write,
                        missing_path_behavior: None,
                    },
                    FileSystemSandboxEntry {
                        path: FileSystemPath::GlobPattern {
                            pattern: request_cwd.join("**/*.env").to_string_lossy().into_owned(),
                        },
                        access: FileSystemAccessMode::Deny,
                        missing_path_behavior: None,
                    },
                ],
                glob_scan_max_depth: std::num::NonZeroUsize::new(2),
            }),
            ..Default::default()
        }
    );
    assert_eq!(
        intersect_permission_profiles(
            PermissionProfile {
                file_system: Some(FileSystemPermissions::from_read_write_roots(
                    /*read*/ None,
                    Some(vec![later_cwd.join("token.env")]),
                )),
                ..Default::default()
            },
            intersected,
            later_cwd.as_path(),
        ),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_drops_broader_cwd_grant_for_requested_child_path() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let child = cwd.join("child");
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            /*read*/ None,
            Some(vec![child]),
        )),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::project_roots(/*subpath*/ None),
                },
                access: FileSystemAccessMode::Write,
                missing_path_behavior: None,
            }],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted, cwd.as_path()),
        PermissionProfile::default()
    );
}

#[test]
fn intersect_permission_profiles_uses_granted_bounded_glob_scan_depth() {
    let cwd = std::env::current_dir().expect("current dir");
    let root_write = FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Write,
        missing_path_behavior: None,
    };
    let deny_env_files = FileSystemSandboxEntry {
        path: FileSystemPath::GlobPattern {
            pattern: "**/*.env".to_string(),
        },
        access: FileSystemAccessMode::Deny,
        missing_path_behavior: None,
    };
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![root_write.clone(), deny_env_files.clone()],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        }),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![root_write.clone(), deny_env_files],
            glob_scan_max_depth: std::num::NonZeroUsize::new(4),
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted, cwd.as_path()),
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![
                    root_write,
                    FileSystemSandboxEntry {
                        path: FileSystemPath::GlobPattern {
                            pattern: AbsolutePathBuf::resolve_path_against_base(
                                "**/*.env",
                                cwd.as_path()
                            )
                            .to_string_lossy()
                            .into_owned(),
                        },
                        access: FileSystemAccessMode::Deny,
                        missing_path_behavior: None,
                    },
                ],
                glob_scan_max_depth: std::num::NonZeroUsize::new(4),
            }),
            ..Default::default()
        }
    );
}

#[test]
fn intersect_permission_profiles_uses_granted_unbounded_glob_scan_depth() {
    let cwd = std::env::current_dir().expect("current dir");
    let root_write = FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Write,
        missing_path_behavior: None,
    };
    let deny_env_files = FileSystemSandboxEntry {
        path: FileSystemPath::GlobPattern {
            pattern: "**/*.env".to_string(),
        },
        access: FileSystemAccessMode::Deny,
        missing_path_behavior: None,
    };
    let requested = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![root_write.clone(), deny_env_files.clone()],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        }),
        ..Default::default()
    };
    let granted = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            entries: vec![root_write.clone(), deny_env_files],
            glob_scan_max_depth: None,
        }),
        ..Default::default()
    };

    assert_eq!(
        intersect_permission_profiles(requested, granted, cwd.as_path()),
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                entries: vec![
                    root_write,
                    FileSystemSandboxEntry {
                        path: FileSystemPath::GlobPattern {
                            pattern: AbsolutePathBuf::resolve_path_against_base(
                                "**/*.env",
                                cwd.as_path()
                            )
                            .to_string_lossy()
                            .into_owned(),
                        },
                        access: FileSystemAccessMode::Deny,
                        missing_path_behavior: None,
                    },
                ],
                glob_scan_max_depth: None,
            }),
            ..Default::default()
        }
    );
}

#[test]
fn merge_file_system_policy_with_additional_permissions_preserves_unreadable_roots() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let allowed_path = cwd.join("allowed");
    let denied_path = cwd.join("denied");
    let merged_policy = merge_file_system_policy_with_additional_permissions(
        &FileSystemSandboxPolicy::restricted(vec![
            FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
                access: FileSystemAccessMode::Read,
                missing_path_behavior: None,
            },
            FileSystemSandboxEntry {
                path: denied_path.clone().into(),
                access: FileSystemAccessMode::Deny,
                missing_path_behavior: None,
            },
        ]),
        &FileSystemPermissions::from_read_write_roots(
            Some(vec![allowed_path.clone()]),
            Some(Vec::new()),
        ),
    );

    assert_eq!(
        merged_policy.entries.contains(&FileSystemSandboxEntry {
            path: denied_path.into(),
            access: FileSystemAccessMode::Deny,
            missing_path_behavior: None,
        }),
        true
    );
    assert_eq!(
        merged_policy.entries.contains(&FileSystemSandboxEntry {
            path: allowed_path.into(),
            access: FileSystemAccessMode::Read,
            missing_path_behavior: None,
        }),
        true
    );
}

#[test]
fn merge_file_system_policy_with_additional_permissions_carries_bounded_glob_scan_depth() {
    let deny_env_files = FileSystemSandboxEntry {
        path: FileSystemPath::GlobPattern {
            pattern: "**/*.env".to_string(),
        },
        access: FileSystemAccessMode::Deny,
        missing_path_behavior: None,
    };
    let merged_policy = merge_file_system_policy_with_additional_permissions(
        &FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
            missing_path_behavior: None,
        }]),
        &FileSystemPermissions {
            entries: vec![deny_env_files.clone()],
            glob_scan_max_depth: std::num::NonZeroUsize::new(2),
        },
    );

    assert_eq!(merged_policy, {
        let mut policy = FileSystemSandboxPolicy::restricted(vec![
            FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
                access: FileSystemAccessMode::Write,
                missing_path_behavior: None,
            },
            deny_env_files,
        ]);
        policy.glob_scan_max_depth = Some(2);
        policy
    });
}

#[test]
fn effective_file_system_sandbox_policy_returns_base_policy_without_additional_permissions() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let denied_path = cwd.join("denied");
    let base_policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
            missing_path_behavior: None,
        },
        FileSystemSandboxEntry {
            path: denied_path.into(),
            access: FileSystemAccessMode::Deny,
            missing_path_behavior: None,
        },
    ]);

    let effective_policy =
        effective_file_system_sandbox_policy(&base_policy, /*additional_permissions*/ None);

    assert_eq!(effective_policy, base_policy);
}

#[test]
fn effective_file_system_sandbox_policy_merges_additional_write_roots() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = AbsolutePathBuf::from_absolute_path(
        canonicalize(temp_dir.path()).expect("canonicalize temp dir"),
    )
    .expect("absolute temp dir");
    let allowed_path = cwd.join("allowed");
    let denied_path = cwd.join("denied");
    let base_policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
            missing_path_behavior: None,
        },
        FileSystemSandboxEntry {
            path: denied_path.clone().into(),
            access: FileSystemAccessMode::Deny,
            missing_path_behavior: None,
        },
    ]);
    let additional_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![]),
            Some(vec![allowed_path.clone()]),
        )),
        ..Default::default()
    };

    let effective_policy =
        effective_file_system_sandbox_policy(&base_policy, Some(&additional_permissions));

    assert_eq!(
        effective_policy.entries.contains(&FileSystemSandboxEntry {
            path: denied_path.into(),
            access: FileSystemAccessMode::Deny,
            missing_path_behavior: None,
        }),
        true
    );
    assert_eq!(
        effective_policy.entries.contains(&FileSystemSandboxEntry {
            path: allowed_path.into(),
            access: FileSystemAccessMode::Write,
            missing_path_behavior: None,
        }),
        true
    );
}
