use super::session::Session;
use crate::realtime_history::RealtimeEventEffects;
use codex_history::RolloutItem;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::RealtimeConversationRealtimeEvent;
use codex_protocol::protocol::RealtimeEvent;
use codex_protocol::realtime::RealtimeItemContent;

impl Session {
    pub(super) async fn send_realtime_history_effects(
        &self,
        submission_id: &str,
        effects: RealtimeEventEffects,
    ) -> anyhow::Result<()> {
        let event = |payload| Event {
            id: submission_id.to_string(),
            msg: EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                payload,
            }),
        };
        if let Some(stream) = effects.transcript_stream {
            if let Some(item) = stream.started_item {
                self.deliver_event_raw(event(RealtimeEvent::HistoryItemStarted(item)))
                    .await;
            }
            self.deliver_event_raw(event(RealtimeEvent::HistoryTranscriptDelta {
                item_id: stream.item_id,
                delta: stream.delta,
            }))
            .await;
        }
        if effects.items.is_empty() {
            return Ok(());
        }
        self.live_thread_for_persistence("append realtime history")?
            .append_items(
                &effects
                    .items
                    .iter()
                    .cloned()
                    .map(RolloutItem::RealtimeItem)
                    .collect::<Vec<_>>(),
            )
            .await?;
        for item in effects.items {
            if !matches!(&item.content, RealtimeItemContent::TranscriptSegment { .. }) {
                self.deliver_event_raw(event(RealtimeEvent::HistoryItemStarted(item.clone())))
                    .await;
            }
            self.deliver_event_raw(event(RealtimeEvent::HistoryItemCompleted(item)))
                .await;
        }
        Ok(())
    }
}
