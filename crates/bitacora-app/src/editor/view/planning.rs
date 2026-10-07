//! The date picker of the `SCHEDULED` / `DEADLINE` chips of the page outline (BIT-US-0167).
//!
//! The editor keeps which chip is open; the chip and the calendar are drawn by
//! `views::planning`. Picking or removing a date is one `SetText` command, so it is a single
//! undo step and only the planning line of the block changes.

use std::rc::Rc;

use bitacora_core::date::Date;
use bitacora_core::editor::{BlockId, Cmd};
use bitacora_markdown::edit::state::{clear_planning, move_planning_date};

use super::{Context, Entity, OutlineEditor, Window};
use crate::ui::App;
use crate::views::calendar::{Month, shift_month};
use crate::views::planning::{OpenPicker, PlanningActions, chip_date, split_key};

impl OutlineEditor {
    /// The text of block `id` in the latest snapshot.
    fn block_text(&self, id: BlockId) -> Option<String> {
        let snap = self.key.as_ref().and_then(|k| self.queue.snapshot(k))?;
        snap.blocks
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.text.clone())
    }

    /// The picker open on a chip, with the block it belongs to.
    #[must_use]
    pub fn planning_open(&self) -> Option<(BlockId, OpenPicker)> {
        self.planning
    }

    /// Opens the picker of `keyword` on block `id`, on the month of its current date.
    pub fn open_planning(&mut self, id: BlockId, keyword: &'static str, cx: &mut Context<Self>) {
        self.flush(cx);
        let today = (self.clock)().map(|(d, _)| d);
        let current = self.block_text(id).and_then(|text| {
            text.lines()
                .find_map(|l| l.split_once(keyword))
                .and_then(|(_, rest)| chip_date(rest.trim_start()))
        });
        let month = match (current, today) {
            (Some((y, m, _)), _) => Month {
                year: y,
                month: u8::try_from(m).unwrap_or(1),
            },
            (None, Some(t)) => Month::of(t),
            (None, None) => return,
        };
        self.planning = Some((id, OpenPicker { keyword, month }));
        cx.notify();
    }

    /// Closes the picker.
    pub fn close_planning(&mut self, cx: &mut Context<Self>) {
        if self.planning.take().is_some() {
            cx.notify();
        }
    }

    /// The month arrows of the open picker.
    pub fn shift_planning(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some((_, open)) = self.planning.as_mut() {
            open.month = shift_month(open.month, delta);
            cx.notify();
        }
    }

    /// Writes the day `key` (`yyyyMMdd`) into the open chip's line.
    pub fn pick_planning(&mut self, key: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, open)) = self.planning else {
            return;
        };
        let (y, m, d) = split_key(key);
        self.rewrite_planning(id, "Reschedule", window, cx, |text| {
            move_planning_date(text, open.keyword, y, m, d)
        });
    }

    /// Removes the open chip's date.
    pub fn clear_planning_date(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, open)) = self.planning else {
            return;
        };
        self.rewrite_planning(id, "Remove date", window, cx, |text| {
            clear_planning(text, open.keyword)
        });
    }

    fn rewrite_planning(
        &mut self,
        id: BlockId,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        rewrite: impl Fn(&str) -> String,
    ) {
        self.flush(cx);
        self.planning = None;
        if let Some(text) = self.block_text(id) {
            let new = rewrite(&text);
            if new != text {
                // The edit buffer would keep the old text: leave edit mode (core has the
                // flushed text already).
                if self.editing() == Some(id) {
                    self.exit_edit_silent();
                }
                self.run(label, Cmd::SetText { id, text: new }, window, cx);
            }
        }
        cx.notify();
    }

    /// The chip actions of the row of block `id`.
    pub(super) fn planning_actions(
        editor: &Entity<Self>,
        id: BlockId,
        cx: &App,
    ) -> PlanningActions {
        let this = editor.read(cx);
        let open = this.planning.filter(|(b, _)| *b == id).map(|(_, o)| o);
        let today: Option<Date> = (this.clock)().map(|(d, _)| d);
        let ed = editor.clone();
        let on_open = {
            let ed = ed.clone();
            Rc::new(move |kw: &'static str, _: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.open_planning(id, kw, cx));
            })
        };
        let on_shift = {
            let ed = ed.clone();
            Rc::new(move |delta: i32, _: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.shift_planning(delta, cx));
            })
        };
        let on_pick = {
            let ed = ed.clone();
            Rc::new(
                move |_: &'static str, key: u32, window: &mut Window, cx: &mut App| {
                    ed.update(cx, |this, cx| this.pick_planning(key, window, cx));
                },
            )
        };
        let on_clear = {
            let ed = ed.clone();
            Rc::new(move |_: &'static str, window: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.clear_planning_date(window, cx));
            })
        };
        let on_close = Rc::new(move |_: &mut Window, cx: &mut App| {
            ed.update(cx, |this, cx| this.close_planning(cx));
        });
        PlanningActions {
            open,
            today,
            on_open,
            on_shift,
            on_pick,
            on_clear,
            on_close,
        }
    }
}
