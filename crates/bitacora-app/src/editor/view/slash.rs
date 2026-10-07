//! Running the slash and angle commands, the template insertion and the calendar popup of the
//! block editor (BIT-US-0105). The pure edits are in [`crate::editor::commands`].

use std::ops::Range;
use std::rc::Rc;

use bitacora_core::date::Date;
use bitacora_core::editor::{Cmd, TemplateContext};
use bitacora_core::journal::journal_page;

use super::{Context, Entity, OutlineEditor, Selection, Window};
use crate::editor::commands::{self, Applied, Command, PickerKind, Rewrite};
use crate::editor::completion::MAX_ITEMS;
use crate::editor::element::PopupData;
use crate::ui::calendar::{self, CalendarEvent, CalendarState};
use crate::ui::{AppContext as _, PathPromptOptions, Subscription};

/// Local date and `HH:mm` time; the editor reads it when a command needs "now".
pub type Clock = Rc<dyn Fn() -> Option<(Date, String)>>;

/// The system clock.
pub fn system_clock() -> Clock {
    Rc::new(|| {
        let now = jiff::Zoned::now();
        let date = Date::new(
            i32::from(now.year()),
            u8::try_from(now.month()).ok()?,
            u8::try_from(now.day()).ok()?,
        )?;
        Some((date, format!("{:02}:{:02}", now.hour(), now.minute())))
    })
}

/// The open calendar.
pub struct DatePick {
    /// What the chosen day is written as.
    pub kind: PickerKind,
    /// Where in the block text a page link goes (the place the trigger was).
    pub at: usize,
    /// The highlighted day.
    pub day: Date,
    /// Calendar widget state.
    pub state: Entity<CalendarState>,
    _sub: Subscription,
}

impl std::fmt::Debug for DatePick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatePick")
            .field("kind", &self.kind)
            .field("at", &self.at)
            .field("day", &self.day)
            .finish_non_exhaustive()
    }
}

fn ymd(d: Date) -> (i32, u32, u32) {
    (d.year(), u32::from(d.month()), u32::from(d.day()))
}

impl OutlineEditor {
    /// Replaces the clock (tests use a fixed day).
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    /// Dates, time and page title for the commands and templates.
    fn command_context(&self) -> Option<TemplateContext> {
        let (date, time) = (self.clock)()?;
        let title = self
            .outline
            .as_ref()
            .map(|o| o.snapshot().title.clone())
            .unwrap_or_default();
        Some(TemplateContext::new(date, &time, &self.config, &title))
    }

    /// What the popup under the caret shows: a window of the candidates, or the calendar.
    pub(super) fn popup_data(&self) -> Option<PopupData> {
        if let Some(p) = &self.picker {
            return Some(PopupData {
                labels: Vec::new(),
                selected: 0,
                first: 0,
                x: self.caret_x(),
                calendar: Some(p.state.clone()),
            });
        }
        let c = self.completion()?;
        let first = (c.selected + 1).saturating_sub(MAX_ITEMS);
        Some(PopupData {
            labels: c
                .items
                .iter()
                .skip(first)
                .take(MAX_ITEMS)
                .map(super::Item::label)
                .collect(),
            selected: c.selected - first,
            first,
            x: self.caret_x(),
            calendar: None,
        })
    }

    /// Replaces the whole visible text and puts the caret in place.
    fn apply_rewrite(&mut self, r: Rewrite, cx: &mut Context<Self>) {
        self.edit_buffer(cx, |b| {
            let len = b.text().len();
            b.replace_range(0..len, &r.text);
            b.set_cursor(r.cursor);
        });
    }

    /// Runs the menu entry `cmd` in place of the trigger text `range`.
    pub(super) fn run_command(
        &mut self,
        cmd: Command,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = &self.edit else { return };
        let text = e.buf.text().to_owned();
        let Some(ctx) = self.command_context() else {
            self.notice(window, cx, rust_i18n::t!("editor.date_unavailable"));
            return;
        };
        match commands::apply(cmd.id, &text, &range, &ctx) {
            Applied::Rewrite(r) => self.apply_rewrite(r, cx),
            Applied::Upload(r) => {
                self.apply_rewrite(r, cx);
                self.pick_assets(window, cx);
            }
            Applied::Picker(kind) => {
                // A date link goes where the trigger was; planning lines do not need the gap.
                let (rest, at) = if kind == PickerKind::Date {
                    let at = range.start.min(text.len());
                    let mut rest = text;
                    rest.replace_range(at..range.end.min(rest.len()), "");
                    (rest, at)
                } else {
                    commands::remove_trimmed(&text, &range)
                };
                self.apply_rewrite(
                    Rewrite {
                        text: rest,
                        cursor: at,
                    },
                    cx,
                );
                self.open_picker(kind, at, window, cx);
            }
            Applied::Templates => {
                let any = self
                    .handle
                    .as_ref()
                    .is_some_and(|h| h.reader.templates().is_ok_and(|t| !t.is_empty()));
                if any {
                    // The menu turns into the template list: `/template ` keeps it open.
                    let start = range.start.min(text.len());
                    let mut next = text;
                    next.replace_range(start..range.end.min(next.len()), "/template ");
                    let cursor = start + "/template ".len();
                    self.dismissed = None;
                    self.apply_rewrite(Rewrite { text: next, cursor }, cx);
                } else {
                    let (rest, at) = commands::remove_trimmed(&text, &range);
                    self.apply_rewrite(
                        Rewrite {
                            text: rest,
                            cursor: at,
                        },
                        cx,
                    );
                    self.notice(window, cx, rust_i18n::t!("editor.no_templates"));
                }
            }
        }
    }

    /// Inserts the template `name` in place of the trigger text `range` (one transaction).
    pub(super) fn insert_template(
        &mut self,
        name: &str,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(handle), Some(ctx), Some(id)) =
            (self.handle.clone(), self.command_context(), self.editing())
        else {
            return;
        };
        // The template may live on a page core has not loaded yet.
        if let Some((_, page)) = handle
            .reader
            .templates()
            .unwrap_or_default()
            .into_iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            crate::editor::ensure_loaded(&self.queue, &handle, &self.config, &page);
        }
        let Some(e) = &self.edit else { return };
        let trigger = e.proj.visible_to_full(e.buf.text(), range.start)
            ..e.proj.visible_to_full(e.buf.text(), range.end);
        let cursor = self.full_selection();
        let Some(tx) = self.run(
            "Insert template",
            Cmd::InsertTemplate {
                target: id,
                trigger,
                name: name.to_owned(),
                ctx,
            },
            window,
            cx,
        ) else {
            return;
        };
        self.after_command(
            Some(id),
            cursor,
            Selection::default(),
            tx.cursor_after,
            window,
            cx,
        );
    }

    // ---- calendar ----------------------------------------------------------------------

    fn open_picker(
        &mut self,
        kind: PickerKind,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((today, _)) = (self.clock)() else {
            return;
        };
        let state = cx.new(|cx| CalendarState::new(window, cx));
        state.update(cx, |s, cx| {
            if let Some(v) = calendar::date_value(
                today.year(),
                u32::from(today.month()),
                u32::from(today.day()),
            ) {
                s.apply_date(v);
            }
            cx.notify();
        });
        let sub = cx.subscribe_in(
            &state,
            window,
            |this, _, event: &CalendarEvent, window, cx| {
                let CalendarEvent::Selected(date) = event;
                if let Some((y, m, d)) = calendar::ymd(date)
                    && let Some(day) = Date::new(
                        y,
                        u8::try_from(m).unwrap_or(1),
                        u8::try_from(d).unwrap_or(1),
                    )
                {
                    this.pick_date(day, window, cx);
                }
            },
        );
        self.picker = Some(DatePick {
            kind,
            at,
            day: today,
            state,
            _sub: sub,
        });
        if let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id)) {
            cx.emit(super::EditorEvent::Row(r));
        }
        cx.notify();
    }

    /// Moves the highlighted day of the open calendar by `days`.
    pub(super) fn move_picker(&mut self, days: i64, cx: &mut Context<Self>) {
        let Some(p) = &mut self.picker else { return };
        let Some(day) = p.day.add_days(days) else {
            return;
        };
        p.day = day;
        let (y, m, d) = ymd(day);
        p.state.update(cx, |s, cx| {
            if let Some(v) = calendar::date_value(y, m, d) {
                s.apply_date(v);
            }
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn on_picker_previous(
        &mut self,
        _: &crate::editor::actions::PickerPreviousDay,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_picker(-1, cx);
    }

    pub(super) fn on_picker_next(
        &mut self,
        _: &crate::editor::actions::PickerNextDay,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_picker(1, cx);
    }

    /// Enter in the calendar: writes the highlighted day.
    pub(super) fn accept_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(day) = self.picker.as_ref().map(|p| p.day) {
            self.pick_date(day, window, cx);
        }
    }

    /// Writes `day` as the open calendar asked and closes it.
    pub fn pick_date(&mut self, day: Date, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        let Some(e) = &self.edit else { return };
        let text = e.buf.text().to_owned();
        let title = journal_page(day, &self.config).title;
        let r = commands::apply_date(p.kind, &text, p.at, ymd(day), &title);
        self.apply_rewrite(r, cx);
        self.focus_handle.focus(window, cx);
    }

    // ---- upload ------------------------------------------------------------------------

    /// `/upload an asset`: the native file dialog; the chosen files are attached to the block.
    fn pick_assets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            // A cancelled dialog, a closed channel or a portal error all mean "nothing chosen".
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.drop_files_on_page(&paths, window, cx);
                });
            }
        })
        .detach();
    }
}
