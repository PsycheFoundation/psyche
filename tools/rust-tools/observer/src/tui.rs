use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use psyche_tui::{CustomWidget, LoggerWidget, init_terminal};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    symbols,
    widgets::{Block, Borders, Tabs},
};
use strum::IntoEnumIterator;

use crate::app::{App, DetailPanel, EventCategory};
use crate::widgets::{
    batches::BatchesWidget, coordinator_bar::CoordinatorBarWidget, event_detail::EventDetailWidget,
    filter_bar::FilterBarWidget, loss_graph::LossGraphWidget, node::NodeWidget,
    scrubber::ScrubberWidget, waterfall::WaterfallWidget,
};

/// Everything the draw closure needs, borrowed immutably from `App`.
struct RenderCtx<'a> {
    snapshot: &'a psyche_event_sourcing::projection::ClusterSnapshot,
    cursor: usize,
    selected_node_idx: Option<usize>,
    selected_node_id: Option<String>,
    node_scroll: usize,
    file_stats: &'a indexmap::IndexMap<String, crate::app::NodeFileStats>,
    detail_panel: DetailPanel,
    timeline: &'a psyche_event_sourcing::timeline::ClusterTimeline,
    all_node_ids: &'a [String],
    waterfall_zoom: usize,
    waterfall_x_scroll: usize,
    waterfall_filter: &'a std::collections::HashSet<EventCategory>,
}

pub fn run(mut app: App) -> anyhow::Result<()> {
    let mut terminal = init_terminal()?;
    let mut logger_widget = LoggerWidget::new();

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();
    let mut timeline_h: usize = 20;

    loop {
        app.ensure_node_visible(timeline_h);
        app.ensure_cursor_visible();

        {
            // Sync the projection (mutable), then borrow everything immutably.
            app.sync_projection();

            let all_node_ids = app.timeline.all_entity_ids();
            let ctx = RenderCtx {
                snapshot: app.snapshot(),
                cursor: app.cursor,
                selected_node_idx: app.selected_node_idx,
                selected_node_id: app
                    .selected_node_idx
                    .and_then(|i| all_node_ids.get(i).cloned()),
                node_scroll: app.node_scroll,
                file_stats: &app.node_file_stats,
                detail_panel: app.detail_panel,
                timeline: &app.timeline,
                all_node_ids,
                waterfall_zoom: app.waterfall_zoom,
                waterfall_x_scroll: app.waterfall_x_scroll,
                waterfall_filter: &app.waterfall_filter,
            };

            terminal.0.draw(|f| {
                let area = f.area();

                // Compute timeline height: at most 1/3 of total area, min 4 rows.
                let total_h = area.height;
                let max_timeline_h = (total_h / 3).max(4);
                // Actual rows needed: 1 ruler + 1 "all info" + node count + 2 scroll indicators.
                let node_count = ctx.all_node_ids.len() as u16;
                let desired_timeline_h = (1 + 1 + node_count + 2).min(max_timeline_h).max(4);

                let outer = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),                  // scrubber
                        Constraint::Length(1),                  // separator ═══
                        Constraint::Length(2),                  // coordinator bar
                        Constraint::Length(desired_timeline_h), // timeline (always visible)
                        Constraint::Min(4),                     // detail panel
                    ])
                    .split(area);

                let scrubber_area = outer[0];
                let sep_area = outer[1];
                let coord_area = outer[2];
                let timeline_area = outer[3];
                let detail_area = outer[4];

                // Store the usable timeline row count (minus ruler and scroll indicator rows).
                timeline_h = timeline_area.height.saturating_sub(3) as usize;

                // ── Scrubber ──────────────────────────────────────────────────
                f.render_widget(
                    ScrubberWidget {
                        timeline: ctx.timeline,
                        cursor: ctx.cursor,
                    },
                    scrubber_area,
                );

                // ── Separator / category filter picker ════════════════════════
                f.render_widget(
                    FilterBarWidget {
                        filter: ctx.waterfall_filter,
                    },
                    sep_area,
                );

                // ── Coordinator bar ───────────────────────────────────────────
                f.render_widget(
                    CoordinatorBarWidget {
                        snapshot: ctx.snapshot,
                    },
                    coord_area,
                );

                // ── Timeline / Waterfall (always visible, not selectable) ────
                f.render_widget(
                    WaterfallWidget {
                        timeline: ctx.timeline,
                        cursor: ctx.cursor,
                        node_ids: ctx.all_node_ids,
                        selected_node_idx: ctx.selected_node_idx,
                        node_scroll: ctx.node_scroll,
                        zoom: ctx.waterfall_zoom,
                        x_scroll: ctx.waterfall_x_scroll,
                        filter: ctx.waterfall_filter,
                    },
                    timeline_area,
                );

                // ── Detail panel (tabbed, selectable) ─────────────────────────
                let detail_border_color = Color::Cyan;
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(detail_border_color));
                let inner = block.inner(detail_area);
                f.render_widget(block, detail_area);

                let snapshot_node_idx = ctx
                    .selected_node_id
                    .as_deref()
                    .and_then(|id| ctx.snapshot.nodes.get_index_of(id));

                match ctx.detail_panel {
                    DetailPanel::Node => f.render_widget(
                        NodeWidget {
                            snapshot: ctx.snapshot,
                            selected_node_idx: snapshot_node_idx,
                            file_stats: ctx.file_stats,
                        },
                        inner,
                    ),
                    DetailPanel::Loss => f.render_widget(
                        LossGraphWidget {
                            nodes: &ctx.snapshot.nodes,
                        },
                        inner,
                    ),
                    DetailPanel::Batches => f.render_widget(
                        BatchesWidget {
                            snapshot: ctx.snapshot,
                            selected_node_id: ctx.selected_node_id.as_deref(),
                        },
                        inner,
                    ),
                    DetailPanel::Event => f.render_widget(
                        EventDetailWidget {
                            timeline: ctx.timeline,
                            cursor: ctx.cursor,
                            selected_node_id: ctx.selected_node_id.as_deref(),
                            filter: ctx.waterfall_filter,
                        },
                        inner,
                    ),
                    DetailPanel::Logs => {
                        logger_widget.render(inner, f.buffer_mut(), &());
                    }
                }

                // Tabs overlay on detail panel top border
                let tab_idx = DetailPanel::iter()
                    .position(|p| p == ctx.detail_panel)
                    .unwrap();
                f.render_widget(
                    Tabs::new(
                        DetailPanel::iter()
                            .enumerate()
                            .map(|(i, p)| format!("[{}] {}", i + 1, p.to_string().to_uppercase())),
                    )
                    .select(tab_idx)
                    .style(Style::default().fg(Color::DarkGray))
                    .highlight_style(Style::default().yellow().bold())
                    .divider(symbols::DOT)
                    .padding(" ", " "),
                    ratatui::layout::Rect {
                        x: detail_area.x + 1,
                        y: detail_area.y,
                        width: detail_area.width.saturating_sub(2),
                        height: 1,
                    },
                );
            })?;
        }

        // ── Input ─────────────────────────────────────────────────────────────
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            let ev = event::read()?;

            // Forward UI events to the logger widget when Logs panel is active.
            if app.detail_panel == DetailPanel::Logs {
                logger_widget.on_ui_event(&ev);
            }

            if let Event::Key(key) = ev {
                // Category filter keys — match dynamically from the enum.
                let cat = EventCategory::iter()
                    .filter(|c| c.filterable())
                    .find(|c| KeyCode::Char(c.key()) == key.code);
                if let Some(cat) = cat {
                    app.toggle_category_filter(cat);
                    continue;
                }

                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) => break,

                    // ── Playback speed (Shift+1/2/3) ─────────────────────────
                    (KeyCode::Char('!'), _) => app.set_speed(1),
                    (KeyCode::Char('@'), _) => app.set_speed(5),
                    (KeyCode::Char('#'), _) => app.set_speed(20),

                    // ── Scrub ────────────────────────────────────────────────
                    (KeyCode::Char('g') | KeyCode::Home, KeyModifiers::NONE) => app.go_first(),
                    (KeyCode::Char('G') | KeyCode::End, _)
                    | (KeyCode::Char('g'), KeyModifiers::SHIFT) => app.go_last(),
                    (KeyCode::Left, KeyModifiers::SHIFT) => app.step_backward(50),
                    (KeyCode::Right, KeyModifiers::SHIFT) => app.step_forward(50),
                    (KeyCode::Left, _) => app.step_backward(1),
                    (KeyCode::Right, _) => app.step_forward(1),

                    // ── ↑/↓ always navigate nodes ────────────────────────────
                    (KeyCode::Up, _) => app.prev_node(),
                    (KeyCode::Down, _) => app.next_node(),

                    // ── Tab still cycles panels ─────────────────────────────
                    (KeyCode::Tab, _) => app.cycle_detail_panel(),

                    // ── Playback / zoom ─────────────────────────────────────
                    (KeyCode::Char(' '), _) => app.toggle_play(),
                    (KeyCode::Char('['), _) => app.zoom_in(),
                    (KeyCode::Char(']'), _) => app.zoom_out(),

                    // ── Box focus (numbers) ─────────────────────────────────
                    (KeyCode::Char(number), KeyModifiers::NONE) => {
                        if let Some(digit) = number.to_digit(10)
                            && let Some(panel) = DetailPanel::iter().nth(digit as usize - 1)
                        {
                            app.switch_panel(panel);
                        }
                    }

                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = Instant::now();
        }
    }

    // Terminal cleanup is handled by TerminalWrapper's Drop impl.
    Ok(())
}
