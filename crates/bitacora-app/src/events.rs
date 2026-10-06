//! Core/service to UI event streaming (ADR-012).
//!
//! Services (tokio tasks, background threads, the future command queue) push events
//! into an `async_channel`; an [`EventPump`] polls the receiver on GPUI's foreground
//! executor and applies each event to a view, then notifies it.

use async_channel::Receiver;

use crate::ui::{Context, Task};

/// Namespace for the pump constructor.
#[derive(Debug)]
pub struct EventPump;

impl EventPump {
    /// Spawns a foreground task that applies every event received on `rx` to the
    /// view being constructed or updated through `cx`, then calls `cx.notify()`.
    ///
    /// The task holds the view weakly and ends when the channel closes (all senders
    /// dropped) or when the view has been released (noticed on the next event).
    /// The view should store the returned [`Task`]: dropping it cancels the pump.
    pub fn attach<V, E>(
        cx: &mut Context<V>,
        rx: Receiver<E>,
        apply: impl Fn(&mut V, E, &mut Context<V>) + 'static,
    ) -> Task<()>
    where
        V: 'static,
        E: 'static,
    {
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                let alive = this.update(cx, |view, cx| {
                    apply(view, event, cx);
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::ui::{AppContext as _, Entity, Render};

    struct Counter {
        seen: Vec<u32>,
        pump: Option<Task<()>>,
    }

    impl Counter {
        fn new(rx: Receiver<u32>, cx: &mut Context<Self>) -> Self {
            let pump = EventPump::attach(cx, rx, |this: &mut Self, event, _| {
                this.seen.push(event);
            });
            Self {
                seen: Vec::new(),
                pump: Some(pump),
            }
        }
    }

    impl Render for Counter {
        fn render(
            &mut self,
            _: &mut crate::ui::Window,
            _: &mut Context<Self>,
        ) -> impl crate::ui::IntoElement {
            crate::ui::div()
        }
    }

    #[gpui_test]
    fn applies_events_in_order(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        for n in 0..5 {
            tx.try_send(n).expect("send");
        }
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |c, _| c.seen.clone()),
            vec![0, 1, 2, 3, 4]
        );
        tx.try_send(5).expect("send");
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |c, _| c.seen.len()), 6);
    }

    #[gpui_test]
    fn pump_ends_without_panic_when_the_view_is_released(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        // Detach the pump so it outlives the view and must notice the release itself.
        view.update(cx, |c, _| {
            if let Some(task) = c.pump.take() {
                task.detach();
            }
        });
        drop(view);
        cx.run_until_parked();
        tx.try_send(1).expect("send");
        cx.run_until_parked();
        // The pump consumed the event, failed to update, and stopped: the channel
        // receiver is gone, so further sends fail.
        assert!(tx.try_send(2).is_err());
    }

    #[gpui_test]
    fn pump_stops_when_all_senders_drop(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded::<u32>();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        drop(tx);
        cx.run_until_parked();
        assert!(view.read_with(cx, |c, _| c.seen.is_empty()));
    }
}
