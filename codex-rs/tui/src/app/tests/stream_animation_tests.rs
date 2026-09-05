use super::*;
use futures::FutureExt;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn stream_animation_preserves_pacing_and_discards_stopped_ticks() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_server = start_config_write_test_app_server(&app).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    let (replacement, _tx, _rx, _ops) = make_chatwidget_manual_with_sender().await;
    time::pause();
    let started_at = time::Instant::now();

    app.handle_event(&mut tui, &mut app_server, AppEvent::StartCommitAnimation)
        .await?;
    time::advance(COMMIT_ANIMATION_TICK / 2).await;
    app.handle_event(&mut tui, &mut app_server, AppEvent::StartCommitAnimation)
        .await?;
    time::advance(COMMIT_ANIMATION_TICK).await;
    let interval = app.commit_animation.as_mut().expect("animation started");
    assert_eq!(
        interval.tick().now_or_never(),
        Some(started_at + COMMIT_ANIMATION_TICK)
    );

    // A blocked UI consumes one overdue tick, not a burst of obsolete ticks.
    time::advance(COMMIT_ANIMATION_TICK * 4).await;
    assert!(interval.tick().now_or_never().is_some());
    assert!(interval.tick().now_or_never().is_none());
    let next_tick = time::Instant::now() + COMMIT_ANIMATION_TICK;
    time::advance(COMMIT_ANIMATION_TICK + Duration::from_millis(/*millis*/ 1)).await;
    assert_eq!(interval.tick().now_or_never(), Some(next_tick));

    // Restart while the previous timer is overdue; only the new deadline survives.
    time::advance(COMMIT_ANIMATION_TICK * 3).await;
    app.handle_event(&mut tui, &mut app_server, AppEvent::StopCommitAnimation)
        .await?;
    assert!(app.commit_animation.is_none());
    app.handle_event(&mut tui, &mut app_server, AppEvent::StartCommitAnimation)
        .await?;
    let interval = app.commit_animation.as_mut().expect("animation restarted");
    assert!(interval.tick().now_or_never().is_none());
    let next_tick = time::Instant::now() + COMMIT_ANIMATION_TICK;
    time::advance(COMMIT_ANIMATION_TICK + Duration::from_millis(/*millis*/ 1)).await;
    assert_eq!(interval.tick().now_or_never(), Some(next_tick));

    app.replace_chat_widget(replacement);
    assert!(app.commit_animation.is_none());
    time::resume();
    Ok(())
}
