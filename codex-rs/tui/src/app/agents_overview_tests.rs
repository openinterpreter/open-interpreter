use super::*;
use crate::app::test_support::make_test_app;
use crate::app_event::AgentsOverviewThreadRefresh;
use crate::bottom_pane::BottomPaneView;
use crate::chatwidget::tests::helpers::render_bottom_popup;
use crate::render::renderable::Renderable;
use crate::test_support::PathBufExt;
use crate::test_support::test_path_buf;
use crate::test_support::test_path_display;
use codex_app_server_client::AppServerEvent;
use codex_app_server_protocol::CurrentTimeReadParams;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadArchivedNotification;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadDeletedNotification;
use codex_app_server_protocol::ThreadNameUpdatedNotification;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStartedNotification;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadUnsubscribeParams;
use codex_app_server_protocol::ThreadUnsubscribeResponse;
use codex_app_server_protocol::ThreadUnsubscribeStatus;
use codex_config::types::KeybindingSpec;
use codex_config::types::KeybindingsSpec;
use codex_config::types::TuiKeymap;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;

static OVERVIEW_TIMESTAMP: std::sync::LazyLock<i64> =
    std::sync::LazyLock::new(|| chrono::Utc::now().timestamp() - 120);

fn overview_thread(
    thread_id: ThreadId,
    parent_thread_id: Option<ThreadId>,
    name: &str,
    status: ThreadStatus,
) -> Thread {
    Thread {
        id: thread_id.to_string(),
        extra: None,
        project_id: None,
        session_id: parent_thread_id.unwrap_or(thread_id).to_string(),
        forked_from_id: None,
        parent_thread_id: None,
        preview: name.to_string(),
        ephemeral: false,
        section: None,
        section_entered_at: None,
        history_mode: Default::default(),
        model_provider: "openai".to_string(),
        model: None,
        reasoning_effort: None,
        created_at: *OVERVIEW_TIMESTAMP,
        updated_at: *OVERVIEW_TIMESTAMP,
        recency_at: Some(*OVERVIEW_TIMESTAMP),
        status,
        path: None,
        cwd: test_path_buf("/tmp/project").abs(),
        cli_version: "0.0.0".to_string(),
        source: parent_thread_id.map_or(SessionSource::Cli, |parent_thread_id| {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
            })
        }),
        can_accept_direct_input: Some(parent_thread_id.is_none()),
        thread_source: None,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: Some(name.to_string()),
        turns: Vec::new(),
    }
}

#[tokio::test]
async fn shared_overview_keeps_rows_and_replays_changes_over_stale_reads() -> Result<()> {
    let mut app = make_test_app().await;
    let app_server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let [retained, archived, deleted, created] = std::array::from_fn(|_| ThreadId::new());
    let [
        mut retained_thread,
        archived_thread,
        deleted_thread,
        created_thread,
    ] = [
        (retained, "Old name"),
        (archived, "Archived"),
        (deleted, "Deleted"),
        (created, "Created in another client"),
    ]
    .map(|(id, name)| {
        overview_thread(id, /*parent_thread_id*/ None, name, ThreadStatus::Idle)
    });
    let request_id = Uuid::new_v4();
    app.agents_overview.request_id = Some(request_id);
    // The seed is still in flight and the command center has never been opened.
    for notification in [
        ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: created_thread.clone(),
        }),
        ServerNotification::ThreadNameUpdated(ThreadNameUpdatedNotification {
            thread_id: retained.to_string(),
            thread_name: Some("New name".to_string()),
        }),
        ServerNotification::ThreadClosed(ThreadClosedNotification {
            thread_id: retained.to_string(),
        }),
        ServerNotification::ThreadArchived(ThreadArchivedNotification {
            thread_id: archived.to_string(),
        }),
        ServerNotification::ThreadDeleted(ThreadDeletedNotification {
            thread_id: deleted.to_string(),
        }),
    ] {
        app.handle_app_server_event(
            &app_server,
            AppServerEvent::ServerNotification(Box::new(notification)),
        )
        .await;
    }
    app.apply_agents_overview_thread_refresh(
        &app_server,
        request_id,
        Ok(AgentsOverviewThreadRefresh {
            threads: HashMap::from([
                (retained, Some(retained_thread.clone())),
                (archived, Some(archived_thread)),
                (deleted, Some(deleted_thread)),
            ]),
            recent_seed_complete: true,
        }),
    );
    retained_thread.name = Some("New name".to_string());
    retained_thread.status = ThreadStatus::NotLoaded;
    let expected = HashMap::from([
        (retained, Some(retained_thread)),
        (created, Some(created_thread)),
    ]);
    assert_eq!(app.agents_overview.threads, expected);

    let request_id = Uuid::new_v4();
    app.agents_overview.request_id = Some(request_id);
    app.apply_agents_overview_thread_refresh(
        &app_server,
        request_id,
        Ok(AgentsOverviewThreadRefresh {
            threads: HashMap::from([(retained, None)]),
            recent_seed_complete: true,
        }),
    );
    assert_eq!(app.agents_overview.threads, expected);
    let request_id = Uuid::new_v4();
    app.agents_overview.request_id = Some(request_id);
    app.apply_agents_overview_thread_refresh(
        &app_server,
        request_id,
        Err("temporarily unavailable".to_string()),
    );
    assert_eq!(app.agents_overview.threads, expected);
    app_server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn shared_overview_seeds_once_and_retains_locally_resumed_history() -> Result<()> {
    let mut app = make_test_app().await;
    let mut ids = Vec::new();
    for day in 1..=22 {
        let source = match day {
            3 => codex_protocol::protocol::SessionSource::Custom("atlas".to_string()),
            4 => codex_protocol::protocol::SessionSource::Custom("chatgpt".to_string()),
            5 => codex_protocol::protocol::SessionSource::Exec,
            6 => codex_protocol::protocol::SessionSource::Mcp,
            _ => codex_protocol::protocol::SessionSource::Cli,
        };
        ids.push(ThreadId::from_string(
            &app_test_support::create_fake_rollout_with_source(
                &app.config.codex_home,
                &format!("2025-01-{day:02}T12-00-00"),
                &format!("2025-01-{day:02}T12:00:00Z"),
                &format!("Task {day}"),
                Some(if day == 10 {
                    "other-provider"
                } else {
                    &app.config.model_provider_id
                }),
                /*git_info*/ None,
                source,
            )
            .expect("materialize historical session"),
        )?);
    }
    let config = app.config.clone();
    let mut app_server = crate::start_embedded_app_server_for_picker(&config).await?;
    for thread_id in [ids[0], ids[21]] {
        app_server
            .resume_thread(
                config.clone(),
                thread_id,
                crate::app_server_session::ResumeModelSettings::PreserveExistingThread,
            )
            .await?;
    }
    app.app_server_target = AppServerTarget::LocalDaemon {
        endpoint: crate::RemoteAppServerEndpoint::UnixSocket {
            socket_path: test_path_buf("/tmp/unused.sock").abs(),
        },
    };
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    app.app_event_tx = crate::app_event_sender::AppEventSender::new(event_tx);
    app.refresh_agents_overview_threads(&app_server);
    finish_overview_refresh(&mut app, &app_server, &mut event_rx).await;
    let recent_ids: HashSet<_> = ids[2..].iter().copied().collect();
    let mut expected = recent_ids.clone();
    expected.insert(ids[0]);
    let retained: HashSet<_> = app.agents_overview.threads.keys().copied().collect();
    assert_eq!(retained, expected);

    let created = app_server.start_thread(&config).await?.session.thread_id;
    // Closing the view must not cancel a metadata refresh or forget unloaded entries.
    app.primary_thread_id = Some(ids[0]);
    app.open_agents_overview(&app_server);
    let visible: HashSet<_> = app
        .agents_overview
        .visible_thread_ids
        .iter()
        .copied()
        .collect();
    assert_eq!(visible, expected);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    finish_overview_refresh(&mut app, &app_server, &mut event_rx).await;
    let retained: HashSet<_> = app.agents_overview.threads.keys().copied().collect();
    assert_eq!(retained, expected);

    // A creation notification adds a session without attaching a local view.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = app_server.next_event().await.expect("server notification");
            let is_created = matches!(&event, AppServerEvent::ServerNotification(notification)
                if matches!(notification.as_ref(), ServerNotification::ThreadStarted(started) if started.thread.id == created.to_string()));
            app.handle_app_server_event(&app_server, event).await;
            if is_created { break; }
        }
    }).await.expect("creation notification received");
    expected.insert(created);
    let retained: HashSet<_> = app.agents_overview.threads.keys().copied().collect();
    assert_eq!(retained, expected);

    // Opening older history is a local addition, even without starting a turn.
    let resumed = app_server
        .resume_thread(
            config.clone(),
            ids[1],
            crate::app_server_session::ResumeModelSettings::PreserveExistingThread,
        )
        .await?;
    app.enqueue_primary_thread_session(resumed.session, resumed.turns)
        .await?;
    app.open_agents_overview(&app_server);
    finish_overview_refresh(&mut app, &app_server, &mut event_rx).await;
    expected.insert(ids[1]);
    let visible: HashSet<_> = app
        .agents_overview
        .visible_thread_ids
        .iter()
        .copied()
        .collect();
    assert_eq!(visible, expected);

    // An unloaded row can be selected through the same flow as a loaded row.
    std::fs::write(
        app.config.codex_home.join("config.toml"),
        "[tui]\nresume_cwd = \"session\"\n",
    )?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    app.select_agents_overview_thread(&mut tui, &mut app_server, ids[2])
        .await?;
    assert_eq!(app.primary_thread_id, Some(ids[2]));
    app_server.shutdown().await?;

    // A fresh TUI/server has no in-memory additions; read-only resumes did not promote history.
    let mut restarted = make_test_app().await;
    restarted.app_server_target = app.app_server_target.clone();
    let app_server = crate::start_embedded_app_server_for_picker(&config).await?;
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    restarted.app_event_tx = crate::app_event_sender::AppEventSender::new(event_tx);
    restarted.refresh_agents_overview_threads(&app_server);
    finish_overview_refresh(&mut restarted, &app_server, &mut event_rx).await;
    let retained: HashSet<_> = restarted.agents_overview.threads.keys().copied().collect();
    assert_eq!(retained, recent_ids);
    app_server.shutdown().await?;
    Ok(())
}

async fn finish_overview_refresh(
    app: &mut App,
    app_server: &AppServerSession,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let AppEvent::AgentsOverviewThreadsLoaded { request_id, result } =
                events.recv().await.expect("overview event")
            {
                assert!(result.is_ok(), "{result:?}");
                app.apply_agents_overview_thread_refresh(app_server, request_id, result);
                return;
            }
        }
    })
    .await
    .expect("overview refresh completed");
}

#[tokio::test]
async fn hidden_system_thread_does_not_refresh_shared_overview() {
    let mut app = make_test_app().await;
    let app_server = crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref())
        .await
        .expect("embedded app server");
    let view = app.agents_overview_view(Vec::new(), /*selected_thread_id*/ None);
    app.chat_widget.show_bottom_pane_view(Box::new(view));

    let request_id = uuid::Uuid::new_v4();
    app.agents_overview.request_id = Some(request_id);

    let parent_thread_id = ThreadId::new();
    app.primary_thread_id = Some(parent_thread_id);
    app.active_thread_id = Some(parent_thread_id);
    app.ensure_thread_channel(parent_thread_id);
    app.agents_overview
        .dispatched_requests
        .insert(parent_thread_id, Vec::new());

    let hidden_thread_id = ThreadId::new();
    let mut thread = overview_thread(
        hidden_thread_id,
        Some(parent_thread_id),
        "Generate thread title",
        ThreadStatus::Idle,
    );
    thread.ephemeral = true;
    thread.thread_source = Some(ThreadSource::Feature("system".to_string()));

    app.handle_app_server_event(
        &app_server,
        AppServerEvent::ServerNotification(Box::new(ServerNotification::ThreadStarted(
            ThreadStartedNotification { thread },
        ))),
    )
    .await;

    assert_eq!(
        (
            app.agents_overview.request_id,
            app.agents_overview.refresh_pending,
        ),
        (Some(request_id), false)
    );
    assert!(!app.thread_event_channels.contains_key(&hidden_thread_id));
    assert!(
        !app.agents_overview
            .dispatched_requests
            .contains_key(&hidden_thread_id)
    );

    let visible_thread_id = ThreadId::new();
    let mut thread = overview_thread(
        visible_thread_id,
        Some(parent_thread_id),
        "Persisted system thread",
        ThreadStatus::Idle,
    );
    thread.thread_source = Some(ThreadSource::Feature("system".to_string()));

    app.handle_app_server_event(
        &app_server,
        AppServerEvent::ServerNotification(Box::new(ServerNotification::ThreadStarted(
            ThreadStartedNotification { thread },
        ))),
    )
    .await;

    assert_eq!(
        (
            app.agents_overview.request_id,
            app.agents_overview.refresh_pending,
        ),
        (Some(request_id), true)
    );
    assert!(app.thread_event_channels.contains_key(&visible_thread_id));
    assert!(
        app.agents_overview
            .dispatched_requests
            .contains_key(&visible_thread_id)
    );

    app_server.shutdown().await.expect("shutdown app server");
}

#[tokio::test]
async fn shared_overview_shows_only_root_sessions() {
    assert_eq!(
        AgentsOverviewGroup::for_status(&ThreadStatus::SystemError),
        AgentsOverviewGroup::NeedsYou
    );
    let mut app = make_test_app().await;
    let first_root = ThreadId::from_string("00000000-0000-0000-0000-000000000101").unwrap();
    let unloaded_root = ThreadId::from_string("00000000-0000-0000-0000-000000000102").unwrap();
    let [child, second_root, side_thread] = std::array::from_fn(|_| ThreadId::new());
    app.primary_thread_id = Some(first_root);

    let mut threads = vec![
        overview_thread(
            first_root,
            /*parent_thread_id*/ None,
            "Build the dashboard",
            ThreadStatus::Idle,
        ),
        overview_thread(
            child,
            Some(first_root),
            "Inspect keyboard shortcuts",
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WaitingOnApproval],
            },
        ),
        overview_thread(
            second_root,
            /*parent_thread_id*/ None,
            "Repair authentication",
            ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        ),
        overview_thread(
            unloaded_root,
            /*parent_thread_id*/ None,
            "Review yesterday's changes",
            ThreadStatus::NotLoaded,
        ),
    ];
    let mut side = overview_thread(
        side_thread,
        /*parent_thread_id*/ None,
        "Ephemeral side",
        ThreadStatus::Idle,
    );
    side.ephemeral = true;
    threads.push(side);
    let view = app.agents_overview_view(threads, /*selected_thread_id*/ None);

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut action_view = AgentsOverviewView::new(
        view.rows.clone(),
        Some(first_root),
        /*exit_on_cancel*/ false,
        crate::app_event_sender::AppEventSender::new(event_tx),
        app.keymap.clone(),
        Arc::clone(&app.agents_overview.view_state),
    );
    let state = &app.agents_overview.view_state;
    assert!(!state.lock().unwrap().status_grouping);
    action_view.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(state.lock().unwrap().status_grouping);
    app.agents_overview_view(Vec::new(), /*selected_thread_id*/ None);
    assert!(state.lock().unwrap().status_grouping);
    assert!(
        action_view.handle_paste("Use \u{1b}[31mthe\u{1b}[0m current project\u{7}".to_string())
    );
    action_view.handle_key_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::DispatchAgentsOverviewTask { prompt, cwd: None })
            if prompt == "Use the current project"
    ));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(action_view.handle_paste("Fix the flaky tests after all retries complete".to_string()));
    let area = ratatui::layout::Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 40, /*height*/ 12,
    );
    let mut buffer = ratatui::buffer::Buffer::empty(area);
    action_view.render(area, &mut buffer);
    let prompt = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(prompt.contains("complete"));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::DispatchAgentsOverviewTask { prompt, cwd: Some(cwd) })
            if prompt == "Fix the flaky tests after all retries complete"
                && cwd == test_path_buf("/tmp/project").abs()
    ));
    assert!(action_view.handle_paste("   ".to_string()));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert!(action_view.handle_paste("Repair authentication".to_string()));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::SelectAgentsOverviewThread { thread_id }) if thread_id == second_root
    ));
    assert!(action_view.handle_paste("Continue working".to_string()));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::DispatchAgentsOverviewTask { prompt, .. })
            if prompt.trim() == "Continue working"
    ));
    action_view.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
    crate::chatwidget::tests::helpers::set_active_cell(
        &mut app.chat_widget,
        Box::new(crate::history_cell::PlainHistoryCell::new(vec![
            Line::from("Previous session status"),
        ])),
    );
    app.chat_widget.show_bottom_pane_view(Box::new(view));
    let project = test_path_display("/tmp/project");
    let normalized_project_group = format!(
        "/tmp/project  3{}",
        " ".repeat(project.len().saturating_sub("/tmp/project".len()))
    );
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(
            "agents_overview",
            render_bottom_popup(&app.chat_widget, /*width*/ 96)
                .replace(&format!("{project}  3"), &normalized_project_group)
                .replace(&project, "/tmp/project")
        );
    });

    let threads = (0..20)
        .map(|index| {
            let thread_id = if index == 0 {
                first_root
            } else {
                ThreadId::new()
            };
            let status = if index == 0 {
                ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::WaitingOnApproval],
                }
            } else {
                ThreadStatus::Idle
            };
            let mut candidate = overview_thread(
                thread_id,
                /*parent_thread_id*/ None,
                &format!("Task {index}"),
                status,
            );
            if index == 0 {
                candidate.name = None;
                candidate.preview = "Inspect unnamed task".to_string();
            }
            candidate.updated_at = index;
            candidate.cwd = if index == 0 {
                test_path_buf("/tmp/project-selected").abs()
            } else {
                test_path_buf(&format!("/tmp/project-{}", index % 3)).abs()
            };
            candidate
        })
        .collect();
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let view = app.agents_overview_view(threads, /*selected_thread_id*/ None);
    app.chat_widget.show_bottom_pane_view(Box::new(view));
    let rendered = render_bottom_popup(&app.chat_widget, /*width*/ 96);
    assert!(
        rendered
            .lines()
            .any(|line| line.contains("› ● Inspect unnamed task  current")
                && line.contains("Needs input"))
    );

    app.transcript_cells.push(std::sync::Arc::new(
        crate::history_cell::PlainHistoryCell::new(vec![ratatui::text::Line::from(
            "Previous conversation",
        )]),
    ));
    let mut tui = crate::tui::test_support::make_test_tui().expect("test terminal");
    let screen_size = tui.terminal.last_known_screen_size;
    app.render_chat_widget_frame(&mut tui, screen_size)
        .expect("render full-screen dashboard");
    assert_eq!(tui.terminal.viewport_area.height, screen_size.height);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.render_chat_widget_frame(&mut tui, screen_size)
        .expect("restore conversation after closing dashboard");
    assert!(tui.terminal.viewport_area.height < screen_size.height);
    assert!(app.last_rendered_history_tail.is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn embedded_sessions_offer_to_start_a_background_server_without_migrating() {
    let mut app = make_test_app().await;
    let app_server = crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref())
        .await
        .expect("embedded app server");

    app.open_agents_overview(&app_server);

    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(
            "agents_overview_embedded",
            render_bottom_popup(&app.chat_widget, /*width*/ 96)
        );
    });
    app_server.shutdown().await.expect("shutdown app server");
}

#[tokio::test]
async fn filtered_dashboard_actions_use_configured_shortcuts() {
    let mut app = make_test_app().await;
    let mut keymap = TuiKeymap::default();
    keymap.agents.search = Some(KeybindingsSpec::One(KeybindingSpec("f6".to_string())));
    keymap.agents.stop = Some(KeybindingsSpec::One(KeybindingSpec("f10".to_string())));
    app.keymap = crate::keymap::RuntimeKeymap::from_config(&keymap).expect("runtime keymap");
    let first = ThreadId::new();
    let second = ThreadId::new();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = AgentsOverviewView::new(
        app.agents_overview_view(
            vec![
                overview_thread(
                    first,
                    /*parent_thread_id*/ None,
                    "First task",
                    ThreadStatus::Idle,
                ),
                overview_thread(
                    second,
                    /*parent_thread_id*/ None,
                    "Second task",
                    ThreadStatus::Active {
                        active_flags: Vec::new(),
                    },
                ),
            ],
            Some(first),
        )
        .rows,
        Some(first),
        /*exit_on_cancel*/ false,
        crate::app_event_sender::AppEventSender::new(event_tx),
        app.keymap.clone(),
        Arc::default(),
    );

    view.handle_key_event(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    assert!(event_rx.try_recv().is_err());
    assert!(view.handle_paste("Do not dispatch this draft".to_string()));
    view.handle_key_event(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE));
    assert!(view.handle_paste("Second task".to_string()));
    view.handle_key_event(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::StopAgentsOverviewThread { thread_id }) if thread_id == second
    ));
    view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        event_rx.try_recv(),
        Ok(AppEvent::SelectAgentsOverviewThread { thread_id }) if thread_id == second
    ));
    assert!(event_rx.try_recv().is_err());
}

#[tokio::test]
async fn failed_root_switch_keeps_background_requests_on_the_active_session() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_server =
        crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref()).await?;
    app.primary_thread_id = Some(ThreadId::new());
    app.ensure_thread_channel(ThreadId::new())
        .store
        .lock()
        .await
        .active_turn_id = Some("running-turn".to_string());
    let mut tui = crate::tui::test_support::make_test_tui()?;

    app.select_agents_overview_thread(&mut tui, &mut app_server, ThreadId::new())
        .await?;

    assert!(app.agents_overview.dispatched_requests.is_empty());
    app_server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn root_switch_preserves_idle_root_with_running_subagent() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_server =
        crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref()).await?;
    let previous = app_server.start_thread(&app.config).await?;
    let previous_root_id = previous.session.thread_id;
    app.enqueue_primary_thread_session(previous.session, previous.turns)
        .await?;
    let target_thread_id = ThreadId::from_string(
        &app_test_support::create_fake_rollout(
            app.config.codex_home.as_path(),
            "2025-01-05T12-00-00",
            "2025-01-05T12:00:00Z",
            "Target task",
            Some(&app.config.model_provider_id),
            /*git_info*/ None,
        )
        .expect("materialize target rollout"),
    )?;
    let target = app_server
        .resume_thread(
            app.config.clone(),
            target_thread_id,
            crate::app_server_session::ResumeModelSettings::PreserveExistingThread,
        )
        .await?;
    let child_id = ThreadId::new();
    let idle_child_id = ThreadId::new();
    app.ensure_thread_channel(idle_child_id);
    app.upsert_agent_picker_thread(
        child_id, /*agent_nickname*/ None, /*agent_role*/ None, /*is_closed*/ false,
    );
    app.agent_navigation.mark_running(child_id);
    app.active_thread_id = Some(child_id);
    let mut tui = crate::tui::test_support::make_test_tui()?;

    app.select_agents_overview_thread(&mut tui, &mut app_server, target.session.thread_id)
        .await?;

    assert!(
        app.agents_overview
            .dispatched_requests
            .contains_key(&idle_child_id)
    );
    app.handle_app_server_event(
        &app_server,
        AppServerEvent::ServerRequest(Box::new(ServerRequest::CurrentTimeRead {
            request_id: RequestId::Integer(99),
            params: CurrentTimeReadParams {
                thread_id: idle_child_id.to_string(),
            },
        })),
    )
    .await;
    assert!(app.agents_overview.dispatched_requests[&idle_child_id].is_empty());
    let response: ThreadUnsubscribeResponse = app_server
        .request_handle()
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::String("verify-root-subscription".to_string()),
            params: ThreadUnsubscribeParams {
                thread_id: previous_root_id.to_string(),
            },
        })
        .await?;
    assert_eq!(response.status, ThreadUnsubscribeStatus::Unsubscribed);
    app_server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn overview_selection_applies_user_permissions_only_to_unloaded_threads() -> Result<()> {
    let mut app = make_test_app().await;
    std::fs::write(
        app.config.codex_home.join("config.toml"),
        "[tui]\nresume_cwd = \"session\"\n",
    )?;
    let mut server_config = app.config.clone();
    server_config
        .permissions
        .set_permission_profile(PermissionProfile::workspace_write())?;
    server_config
        .permissions
        .approval_policy
        .set(codex_protocol::protocol::AskForApproval::Never)?;
    let mut thread_ids = Vec::new();
    for day in 1..=5 {
        thread_ids.push(ThreadId::from_string(
            &app_test_support::create_fake_rollout(
                &server_config.codex_home,
                &format!("2025-01-{day:02}T12-00-00"),
                &format!("2025-01-{day:02}T12:00:00Z"),
                "Historical session",
                Some(&server_config.model_provider_id),
                /*git_info*/ None,
            )
            .expect("create historical session"),
        )?);
    }
    let mut app_server = crate::start_embedded_app_server_for_picker(&server_config).await?;
    let loaded = app_server
        .resume_thread(
            server_config.clone(),
            thread_ids[0],
            crate::app_server_session::ResumeModelSettings::RestoreFromThread,
        )
        .await?
        .session;
    app.harness_overrides.sandbox_mode = Some(codex_protocol::config_types::SandboxMode::ReadOnly);
    app.harness_overrides.approval_policy =
        Some(codex_protocol::protocol::AskForApproval::UnlessTrusted);
    app.config
        .permissions
        .set_permission_profile(PermissionProfile::read_only())?;
    app.runtime_permission_profile_override =
        Some(RuntimePermissionProfileOverride::from_config(&app.config));
    app.runtime_approval_policy_override = Some(RuntimeApprovalPolicyOverride::Explicit(
        AskForApproval::UnlessTrusted,
    ));
    let mut tui = crate::tui::test_support::make_test_tui()?;
    for (thread_id, expected_permissions, expected_approval) in [
        (
            loaded.thread_id,
            loaded.permission_profile,
            loaded.approval_policy,
        ),
        (
            thread_ids[1],
            PermissionProfile::read_only(),
            codex_app_server_protocol::AskForApproval::UnlessTrusted,
        ),
        (
            thread_ids[2],
            PermissionProfile::read_only(),
            codex_app_server_protocol::AskForApproval::UnlessTrusted,
        ),
        (
            thread_ids[3],
            PermissionProfile::read_only(),
            codex_app_server_protocol::AskForApproval::UnlessTrusted,
        ),
    ] {
        if thread_id == thread_ids[2] {
            // A session-only /permissions choice must survive subsequent cold resumes.
            std::fs::write(
                app.config.codex_home.join("config.toml"),
                "approvals_reviewer = \"auto_review\"\n[tui]\nresume_cwd = \"session\"\n",
            )?;
            app.harness_overrides.sandbox_mode =
                Some(codex_protocol::config_types::SandboxMode::WorkspaceWrite);
            app.harness_overrides.approval_policy =
                Some(codex_protocol::protocol::AskForApproval::Never);
            app.runtime_permission_profile_override =
                Some(RuntimePermissionProfileOverride::from_config(&app.config));
            app.runtime_approval_policy_override = Some(RuntimeApprovalPolicyOverride::Explicit(
                AskForApproval::UnlessTrusted,
            ));
        }
        app.select_agents_overview_thread(&mut tui, &mut app_server, thread_id)
            .await?;
        let observed = app_server
            .resume_thread(
                app.config.clone(),
                thread_id,
                crate::app_server_session::ResumeModelSettings::PreserveExistingThread,
            )
            .await?
            .session;
        assert_eq!(
            (
                app.primary_thread_id,
                observed.permission_profile,
                observed.approval_policy,
                observed.approvals_reviewer,
            ),
            (
                Some(thread_id),
                expected_permissions,
                expected_approval,
                ApprovalsReviewer::User
            ),
        );
    }
    let requirements = app.config.codex_home.join("requirements.toml");
    std::fs::write(
        &requirements,
        "allowed_approvals_reviewers = [\"auto_review\"]\n",
    )?;
    app.loader_overrides.system_requirements_path = Some(requirements.to_path_buf());
    app.select_agents_overview_thread(&mut tui, &mut app_server, thread_ids[4])
        .await?;
    assert_eq!(
        (
            app.primary_thread_id,
            app_server
                .thread_read(thread_ids[4], /*include_turns*/ false)
                .await?
                .status
        ),
        (
            Some(thread_ids[3]),
            codex_app_server_protocol::ThreadStatus::NotLoaded
        ),
    );
    app_server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn overview_cold_resume_honors_working_directory_selection() -> Result<()> {
    for (mode, cli_cwd, runtime_cwd) in [
        ("current", false, false),
        ("session", true, false),
        ("session", true, true),
        ("session", false, false),
    ] {
        let mut app = make_test_app().await;
        let chosen = app.config.codex_home.join("chosen");
        let overridden = app.config.codex_home.join("overridden");
        std::fs::create_dir(&chosen)?;
        std::fs::create_dir(&overridden)?;
        std::fs::write(
            app.config.codex_home.join("config.toml"),
            format!("[tui]\nresume_cwd = \"{mode}\"\n"),
        )?;
        crate::legacy_core::config::set_project_trust_level(
            app.config.codex_home.as_path(),
            &chosen,
            codex_protocol::config_types::TrustLevel::Trusted,
        )
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
        let thread_id = ThreadId::from_string(
            &app_test_support::create_fake_rollout(
                &app.config.codex_home,
                "2025-01-01T12-00-00",
                "2025-01-01T12:00:00Z",
                "Historical session",
                Some(&app.config.model_provider_id),
                /*git_info*/ None,
            )
            .expect("create historical session"),
        )?;
        let mut app_server = crate::start_embedded_app_server_for_picker(&app.config).await?;
        app.launch_cwd = chosen.to_path_buf();
        app.harness_overrides.cwd = cli_cwd.then(|| {
            if runtime_cwd {
                overridden.to_path_buf()
            } else {
                chosen.to_path_buf()
            }
        });
        app.runtime_working_directory_override = runtime_cwd.then(|| chosen.to_path_buf());
        let expected_cwd = if mode == "current" || cli_cwd || runtime_cwd {
            chosen
        } else {
            test_path_buf("/").abs()
        };
        let mut tui = crate::tui::test_support::make_test_tui()?;
        app.select_agents_overview_thread(&mut tui, &mut app_server, thread_id)
            .await?;
        let observed = app_server
            .resume_thread(
                app.config.clone(),
                thread_id,
                crate::app_server_session::ResumeModelSettings::PreserveExistingThread,
            )
            .await?
            .session;
        assert_eq!(
            (app.config.cwd.clone(), observed.cwd),
            (expected_cwd.clone(), expected_cwd),
        );
        app_server.shutdown().await?;
    }
    Ok(())
}

#[tokio::test]
async fn restored_server_permission_profile_survives_cd_without_turn_override() -> Result<()> {
    let mut app = make_test_app().await;
    let destination = app.config.codex_home.join("destination");
    std::fs::create_dir(&destination)?;
    crate::legacy_core::config::set_project_trust_level(
        app.config.codex_home.as_path(),
        &destination,
        codex_protocol::config_types::TrustLevel::Trusted,
    )
    .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    let mut app_server =
        crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref()).await?;
    let previous = app_server.start_thread(&app.config).await?;
    app.enqueue_primary_thread_session(previous.session, previous.turns)
        .await?;
    let target = app_server.start_thread(&app.config).await?;
    let target_thread_id = target.session.thread_id;
    let mut tui = crate::tui::test_support::make_test_tui()?;

    app.select_agents_overview_thread(&mut tui, &mut app_server, target_thread_id)
        .await?;
    app.config
        .permissions
        .set_permission_profile(PermissionProfile::read_only())?;
    app.chat_widget.set_permission_profile_with_active_profile(
        PermissionProfile::read_only(),
        /*active_permission_profile*/ None,
    )?;
    app.runtime_permission_profile_override = Some(
        RuntimePermissionProfileOverride::from_restored_config(app.chat_widget.config_ref()),
    );

    assert_eq!(
        app.chat_widget
            .config_ref()
            .permissions
            .effective_permission_profile(),
        PermissionProfile::read_only()
    );
    assert_eq!(
        app.runtime_permission_profile_override
            .as_ref()
            .map(|profile| profile.turn_override),
        Some(RuntimePermissionProfileTurnOverride::Preserve)
    );
    assert_eq!(
        App::turn_permissions_override_from_config(
            app.chat_widget.config_ref(),
            /*active_permission_profile*/ None,
            app.runtime_permission_profile_override
                .as_ref()
                .and_then(RuntimePermissionProfileOverride::turn_permission_profile),
        ),
        TurnPermissionsOverride::Preserve
    );

    app.change_working_directory(&mut tui, &mut app_server, destination.clone())
        .await;

    assert_eq!(app.chat_widget.config_ref().cwd, destination);
    assert_eq!(
        app.chat_widget
            .config_ref()
            .permissions
            .effective_permission_profile(),
        PermissionProfile::read_only()
    );

    app_server.shutdown().await?;
    Ok(())
}
