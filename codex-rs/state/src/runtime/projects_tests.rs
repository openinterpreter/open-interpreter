use std::collections::BTreeMap;

use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use codex_utils_absolute_path::test_support::PathExt;
use pretty_assertions::assert_eq;

use super::*;
use crate::ProjectSortKey;
use crate::SortDirection;
use crate::ThreadMetadataBuilder;
use crate::runtime::test_support::unique_temp_dir;

#[tokio::test]
async fn project_lifecycle_preserves_order_and_clears_assignments() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let thread_id = ThreadId::default();
    let metadata = ThreadMetadataBuilder::new(
        thread_id,
        home.join("thread.jsonl"),
        chrono::Utc::now(),
        SessionSource::Cli,
    )
    .build("test-provider");
    runtime.upsert_thread(&metadata).await?;

    let roots = vec![
        ProjectRoot {
            path: "/tmp/one".to_string(),
        },
        ProjectRoot {
            path: "/tmp/two".to_string(),
        },
    ];
    let project = runtime
        .create_project(
            "Work".to_string(),
            roots.clone(),
            BTreeMap::from([("color".to_string(), "blue".to_string())]),
            &[thread_id.to_string()],
            "state:project-lifecycle",
        )
        .await?
        .project;
    assert_eq!(project.position, 0);
    assert_eq!(project.roots, roots);
    assert_eq!(
        runtime.get_thread(thread_id).await?.unwrap().project_id,
        Some(project.id.clone())
    );

    let unchanged = runtime
        .update_project(
            &project.id,
            /*name*/ None,
            /*roots*/ None,
            /*metadata*/ None,
        )
        .await?
        .unwrap();
    assert!(!unchanged.1);
    assert_eq!(unchanged.0.updated_at_ms, project.updated_at_ms);
    runtime
        .touch_thread_updated_at(thread_id, chrono::Utc::now())
        .await?;
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(project.clone())
    );

    runtime
        .mark_archived(
            thread_id,
            home.join("archived.jsonl").as_path(),
            chrono::Utc::now(),
        )
        .await?;
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(Project {
            recency_at_ms: None,
            ..project.clone()
        })
    );
    runtime
        .mark_unarchived(thread_id, &metadata.rollout_path)
        .await?;
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(project.clone())
    );
    runtime
        .mark_archived(thread_id, &metadata.rollout_path, chrono::Utc::now())
        .await?;
    let active_thread_id = ThreadId::default();
    let active_metadata = ThreadMetadataBuilder::new(
        active_thread_id,
        home.join("active.jsonl"),
        chrono::Utc::now(),
        SessionSource::Cli,
    )
    .build("test-provider");
    runtime.upsert_thread(&active_metadata).await?;
    runtime
        .set_thread_project(&active_thread_id.to_string(), Some(&project.id))
        .await?
        .expect("active thread exists");
    let active_recency = runtime
        .get_thread(active_thread_id)
        .await?
        .unwrap()
        .recency_at
        .timestamp_millis();
    runtime
        .mark_unarchived(thread_id, &metadata.rollout_path)
        .await?;
    let active_project = Project {
        recency_at_ms: Some(active_recency),
        ..project.clone()
    };
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(active_project)
    );
    runtime
        .set_thread_project(&active_thread_id.to_string(), /*project_id*/ None)
        .await?;
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(project.clone())
    );
    runtime
        .set_thread_project(&active_thread_id.to_string(), Some(&project.id))
        .await?;
    runtime.delete_thread(active_thread_id).await?;
    assert_eq!(
        runtime.get_project(&project.id).await?,
        Some(project.clone())
    );
    // Restore the active fixture for the project-deletion assertions below.
    runtime.upsert_thread(&active_metadata).await?;
    runtime
        .set_thread_project(&active_thread_id.to_string(), Some(&project.id))
        .await?;
    runtime
        .mark_archived(thread_id, &metadata.rollout_path, chrono::Utc::now())
        .await?;
    let (affected_active_thread_ids, affected_archived_thread_ids) =
        runtime.delete_project(&project.id).await?.unwrap();
    assert_eq!(
        affected_active_thread_ids,
        vec![active_thread_id.to_string()]
    );
    assert_eq!(affected_archived_thread_ids, vec![thread_id.to_string()]);
    assert_eq!(
        runtime.get_thread(thread_id).await?.unwrap().project_id,
        None
    );
    assert_eq!(
        runtime
            .get_thread(active_thread_id)
            .await?
            .unwrap()
            .project_id,
        None
    );
    assert_eq!(runtime.get_project(&project.id).await?, None);
    Ok(())
}

#[tokio::test]
async fn project_idempotency_keys_replay_and_survive_deletion() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let created = runtime
        .create_project(
            "Imported".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "desktop:legacy-project",
        )
        .await?;
    assert!(created.created);
    assert_eq!(
        runtime
            .get_project_by_idempotency_key("desktop:legacy-project")
            .await?,
        Some(created.project.clone())
    );
    let replayed = runtime
        .create_project(
            "Changed payload".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "desktop:legacy-project",
        )
        .await?;
    assert!(!replayed.created);
    assert_eq!(replayed.project, created.project);

    runtime.delete_project(&created.project.id).await?;
    let lookup_error = runtime
        .get_project_by_idempotency_key("desktop:legacy-project")
        .await
        .expect_err("deleted project keys must remain tombstoned");
    assert!(
        lookup_error
            .to_string()
            .contains("idempotency key refers to deleted project")
    );
    let error = runtime
        .create_project(
            "Recreated".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "desktop:legacy-project",
        )
        .await
        .expect_err("deleted project keys must remain tombstoned");
    assert!(
        error
            .to_string()
            .contains("idempotency key refers to deleted project")
    );
    Ok(())
}

#[tokio::test]
async fn project_import_rejects_unknown_thread_without_partial_project() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let error = runtime
        .create_project(
            "Work".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &["00000000-0000-0000-0000-000000000123".to_string()],
            "state:unknown-thread",
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("thread not found"));
    assert!(
        runtime
            .list_projects(
                /*cursor*/ None,
                /*limit*/ 10,
                ProjectSortKey::Position,
                SortDirection::Asc
            )
            .await?
            .projects
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn project_updates_preserve_omitted_fields_across_concurrent_writers() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime_one = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let runtime_two = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let project = runtime_one
        .create_project(
            "Work".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "state:concurrent-update",
        )
        .await?
        .project;

    let (name_update, metadata_update) = tokio::join!(
        runtime_one.update_project(
            &project.id,
            Some("Renamed".to_string()),
            /*roots*/ None,
            /*metadata*/ None,
        ),
        runtime_two.update_project(
            &project.id,
            /*name*/ None,
            /*roots*/ None,
            Some(BTreeMap::from([("color".to_string(), "blue".to_string())])),
        ),
    );
    name_update?.expect("project exists");
    metadata_update?.expect("project exists");

    let updated = runtime_one
        .get_project(&project.id)
        .await?
        .expect("project exists");
    assert_eq!(updated.name, "Renamed");
    assert_eq!(
        updated.metadata,
        BTreeMap::from([("color".to_string(), "blue".to_string())])
    );
    Ok(())
}

#[tokio::test]
async fn project_list_rejects_malformed_cursors() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;

    for cursor in [
        "",
        "123",
        "+123|00000000-0000-0000-0000-000000000000",
        "000123|00000000-0000-0000-0000-000000000000",
        "-0|00000000-0000-0000-0000-000000000000",
        "-1|00000000-0000-0000-0000-000000000000",
        "123|not-a-uuid",
        "123|00000000000000000000000000000000",
        "123|00000000-0000-0000-0000-00000000000A",
        "123|00000000-0000-0000-0000-000000000000|extra",
        "not-a-position|00000000-0000-0000-0000-000000000000",
    ] {
        let error = runtime
            .list_projects(
                Some(cursor),
                /*limit*/ 10,
                ProjectSortKey::Position,
                SortDirection::Asc,
            )
            .await
            .expect_err("malformed cursor should fail");
        assert!(error.to_string().starts_with("invalid project cursor:"));
    }
    Ok(())
}

#[tokio::test]
async fn project_list_cursor_round_trips_across_pages() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    for name in ["One", "Two"] {
        runtime
            .create_project(name.to_string(), Vec::new(), BTreeMap::new(), &[], name)
            .await?;
    }

    let first = runtime
        .list_projects(
            /*cursor*/ None,
            /*limit*/ 1,
            ProjectSortKey::Position,
            SortDirection::Asc,
        )
        .await?;
    assert_eq!(first.projects.len(), 1);
    let cursor = first.next_cursor.expect("next cursor");
    let second = runtime
        .list_projects(
            Some(&cursor),
            /*limit*/ 1,
            ProjectSortKey::Position,
            SortDirection::Asc,
        )
        .await?;
    assert_eq!(second.projects.len(), 1);
    assert_ne!(first.projects[0].id, second.projects[0].id);
    assert_eq!(second.next_cursor, None);
    Ok(())
}

#[tokio::test]
async fn project_list_orders_and_pages_recency_with_roots_and_nulls() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let mut projects = Vec::new();
    for (name, recency_at_ms, archived) in [
        ("older", Some(1_700_000_000_001), false),
        ("tie-a", Some(1_700_000_000_002), false),
        ("tie-b", Some(1_700_000_000_002), false),
        ("empty", None, false),
        ("archived", Some(1_700_000_010_000), true),
    ] {
        let mut threads = Vec::new();
        if let Some(timestamp) = recency_at_ms {
            for timestamp in [timestamp - 1, timestamp] {
                let id = ThreadId::default();
                let mut metadata = ThreadMetadataBuilder::new(
                    id,
                    home.join(format!("{id}.jsonl")),
                    chrono::DateTime::from_timestamp_millis(timestamp).unwrap(),
                    SessionSource::Exec,
                )
                .build("test-provider");
                metadata.archived_at = archived.then(chrono::Utc::now);
                runtime.upsert_thread(&metadata).await?;
                // Force exact ties despite the monotonic timestamp allocator.
                sqlx::query("UPDATE threads SET recency_at_ms = ?, recency_at = ? WHERE id = ?")
                    .bind(timestamp)
                    .bind(timestamp / 1000)
                    .bind(id.to_string())
                    .execute(runtime.pool.as_ref())
                    .await?;
                threads.push(id.to_string());
            }
        }
        let project = runtime
            .create_project(
                name.to_string(),
                vec![
                    ProjectRoot {
                        path: "/tmp/z".to_string(),
                    },
                    ProjectRoot {
                        path: "/tmp/a".to_string(),
                    },
                ],
                BTreeMap::new(),
                &threads,
                name,
            )
            .await?
            .project;
        let expected = Project {
            recency_at_ms: recency_at_ms.filter(|_| !archived),
            ..project.clone()
        };
        assert_eq!(project, expected);
        assert_eq!(
            runtime.get_project(&project.id).await?,
            Some(expected.clone())
        );
        projects.push(expected);
    }
    let mut ties = projects[1..3].to_vec();
    ties.sort_by(|a, b| a.id.cmp(&b.id));
    let mut empty = projects[3..].to_vec();
    empty.sort_by(|a, b| a.id.cmp(&b.id));
    let ascending = [vec![projects[0].clone()], ties.clone(), empty.clone()].concat();
    ties.reverse();
    empty.reverse();
    let descending = [ties, vec![projects[0].clone()], empty].concat();
    for (key, direction, expected) in [
        (
            ProjectSortKey::Position,
            SortDirection::Asc,
            projects.clone(),
        ),
        (
            ProjectSortKey::Position,
            SortDirection::Desc,
            projects.into_iter().rev().collect(),
        ),
        (ProjectSortKey::RecencyAt, SortDirection::Asc, ascending),
        (
            ProjectSortKey::RecencyAt,
            SortDirection::Desc,
            descending.clone(),
        ),
    ] {
        for limit in [1, 2, 3, 10] {
            let mut cursor = None;
            let mut actual = Vec::new();
            loop {
                let page = runtime
                    .list_projects(cursor.as_deref(), limit, key, direction)
                    .await?;
                actual.extend(page.projects);
                assert!(
                    actual.len() <= expected.len(),
                    "pagination repeated a project"
                );
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            assert_eq!(actual, expected);
        }
    }
    let first = runtime
        .list_projects(
            /*cursor*/ None,
            /*limit*/ 1,
            ProjectSortKey::RecencyAt,
            SortDirection::Desc,
        )
        .await?;
    runtime.delete_project(&first.projects[0].id).await?;
    let rest = runtime
        .list_projects(
            first.next_cursor.as_deref(),
            /*limit*/ 10,
            ProjectSortKey::RecencyAt,
            SortDirection::Desc,
        )
        .await?;
    assert_eq!(
        rest,
        ProjectsPage {
            projects: descending[1..].to_vec(),
            next_cursor: None
        }
    );

    let query = project_list_query(
        /*cursor*/ None,
        /*limit*/ 50,
        ProjectSortKey::RecencyAt,
        SortDirection::Desc,
    )?;
    let plan = QueryBuilder::<Sqlite>::new("EXPLAIN QUERY PLAN ")
        .push(query.sql().as_str())
        .build()
        .bind(/*value*/ 51_i64)
        .fetch_all(runtime.pool.as_ref())
        .await?;
    let details = plan
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>();
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("USING COVERING INDEX idx_threads_project_recency")),
        "{details:?}"
    );
    Ok(())
}

#[test]
fn project_list_rejects_cursor_sort_mismatches() {
    for cursor in [
        "1|00000000-0000-0000-0000-000000000000",
        "v1|position|desc|1|00000000-0000-0000-0000-000000000000",
        "v1|recencyAt|asc|1|00000000-0000-0000-0000-000000000000",
        "v2|recencyAt|desc|1|00000000-0000-0000-0000-000000000000",
        "v1|recencyAt|desc|01|00000000-0000-0000-0000-000000000000",
        "v1|recencyAt|desc|null|not-a-uuid",
    ] {
        assert!(
            project_list_query(
                Some(cursor),
                /*limit*/ 10,
                ProjectSortKey::RecencyAt,
                SortDirection::Desc
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn project_move_reorders_projects_and_preserves_no_op_timestamp() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let one = runtime
        .create_project(
            "One".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "state:move-one",
        )
        .await?
        .project;
    let two = runtime
        .create_project(
            "Two".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "state:move-two",
        )
        .await?
        .project;
    let three = runtime
        .create_project(
            "Three".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "state:move-three",
        )
        .await?
        .project;

    assert_eq!(
        runtime.move_project(&three.id, Some(&one.id)).await?,
        Some(true)
    );
    let reordered = runtime
        .list_projects(
            /*cursor*/ None,
            /*limit*/ 10,
            ProjectSortKey::Position,
            SortDirection::Asc,
        )
        .await?;
    assert_eq!(
        reordered
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        vec![three.id.as_str(), one.id.as_str(), two.id.as_str()]
    );
    assert_eq!(
        reordered
            .projects
            .iter()
            .map(|project| project.position)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    let moved = runtime.get_project(&three.id).await?.unwrap();
    assert_eq!(
        runtime.move_project(&three.id, Some(&one.id)).await?,
        Some(false)
    );
    assert_eq!(
        runtime.get_project(&three.id).await?.unwrap().updated_at_ms,
        moved.updated_at_ms
    );
    assert_eq!(
        runtime
            .move_project(&three.id, Some(&three.id))
            .await
            .unwrap_err()
            .to_string(),
        format!("project {} cannot be moved before itself", three.id)
    );
    assert_eq!(
        runtime
            .move_project(&three.id, Some("00000000-0000-0000-0000-000000000000"))
            .await
            .unwrap_err()
            .to_string(),
        "before project not found: 00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(
        runtime
            .move_project(
                "00000000-0000-0000-0000-000000000000",
                /*before_project_id*/ None,
            )
            .await?,
        None
    );
    Ok(())
}

#[tokio::test]
async fn initial_project_assignment_is_inserted_with_thread_row() -> anyhow::Result<()> {
    let home = unique_temp_dir();
    let runtime = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let project = runtime
        .create_project(
            "Work".to_string(),
            Vec::new(),
            BTreeMap::new(),
            &[],
            "state:initial-assignment",
        )
        .await?
        .project;
    let thread_id = ThreadId::default();
    let mut metadata = ThreadMetadataBuilder::new(
        thread_id,
        home.join("thread.jsonl"),
        chrono::Utc::now(),
        SessionSource::Cli,
    )
    .build("test-provider");
    metadata.project_id = Some(project.id.clone());
    assert!(runtime.insert_thread_if_absent(&metadata).await?);
    assert_eq!(
        runtime.get_thread(thread_id).await?.unwrap().project_id,
        Some(project.id.clone())
    );

    let missing_thread_id = ThreadId::default();
    let mut missing_project_metadata = ThreadMetadataBuilder::new(
        missing_thread_id,
        home.join("missing-project.jsonl"),
        chrono::Utc::now(),
        SessionSource::Cli,
    )
    .build("test-provider");
    missing_project_metadata.project_id = Some("00000000-0000-0000-0000-000000000000".to_string());
    let error = runtime
        .insert_thread_if_absent(&missing_project_metadata)
        .await
        .expect_err("unknown project must reject initial thread insert");
    assert!(error.to_string().contains("FOREIGN KEY constraint failed"));
    assert_eq!(runtime.get_thread(missing_thread_id).await?, None);

    let deleted = runtime.delete_project(&project.id).await?.unwrap();
    assert_eq!(deleted, (vec![thread_id.to_string()], Vec::new()));
    Ok(())
}
