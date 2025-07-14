use crate::{NetworkConnection, Networkable, peer_list::PeerList, util::fmt_bytes};

use iroh::{PublicKey, endpoint::ConnectionType};
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols,
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Padding, Paragraph,
        Widget, Wrap,
    },
};
use std::{
    collections::{HashMap, VecDeque},
    ops::Sub,
    time::Instant,
};

#[derive(Default, Debug)]
pub struct NetworkTui;

impl psyche_tui::CustomWidget for NetworkTui {
    type Data = NetworkTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        if let Some(state) = &state.inner {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        // join ticket
                        Constraint::Max(5),
                        // clients
                        Constraint::Percentage(35),
                        // uploads & download
                        Constraint::Fill(1),
                    ]
                    .as_ref(),
                )
                .split(area);

            // Clients
            {
                Paragraph::new(state.join_ticket.to_string())
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::default()
                            .title("Join Ticket")
                            .padding(Padding::symmetric(1, 0))
                            .borders(Borders::ALL),
                    )
                    .render(chunks[0], buf);

                List::new(state.last_seen.iter().map(
                    |(peer_id, (peer_connection_method, last_seen_instant))| {
                        let last_seen_time = Instant::now().sub(*last_seen_instant).as_secs_f64();
                        let li = ListItem::new(format!(
                            "{} ({}): {:.2} seconds ago",
                            peer_id.fmt_short(),
                            peer_connection_method,
                            last_seen_time
                        ));
                        if last_seen_time < 1.0 {
                            li.bg(Color::LightYellow).fg(Color::Black)
                        } else {
                            li
                        }
                    },
                ))
                .block(
                    Block::default()
                        .title("Recently Seen Peers")
                        .borders(Borders::ALL),
                )
                .render(chunks[1], buf);
            }

            // Upload & Download
            {
                let network_chunks =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(chunks[2]);

                // Downloads and Download Bandwidth
                {
                    let download_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [Constraint::Percentage(30), Constraint::Percentage(70)].as_ref(),
                        )
                        .split(network_chunks[1]);

                    List::new(state.downloads.iter().map(|(hash, download)| {
                        let percent = 100.0 * (download.downloaded as f64 / download.total as f64);
                        ListItem::new(format!(
                            "[{:02.1}%] {}/{}: {}",
                            percent,
                            fmt_bytes(download.downloaded as f64),
                            fmt_bytes(download.total as f64),
                            hash,
                        ))
                    }))
                    .block(
                        Block::default()
                            .title(format!("Downloads ({})", state.downloads.len()))
                            .borders(Borders::ALL),
                    )
                    .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                    .highlight_symbol(">>")
                    .render(download_chunks[0], buf);

                    let bw_history = state
                        .download_bandwidth_history
                        .iter()
                        .enumerate()
                        .map(|(x, y)| (x as f64, *y))
                        .collect::<Vec<_>>();

                    let ymax = bw_history
                        .iter()
                        .map(|(_, y)| *y)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap_or(0.0)
                        .max(1024.0);

                    Chart::new(vec![
                        Dataset::default()
                            .marker(symbols::Marker::Braille)
                            .graph_type(GraphType::Line)
                            .data(&bw_history),
                    ])
                    .block(
                        Block::default()
                            .title(format!(
                                "Download Bandwidth {}/s",
                                fmt_bytes(state.total_data_per_sec)
                            ))
                            .borders(Borders::ALL),
                    )
                    .x_axis(
                        Axis::default()
                            .title("Time")
                            .labels(vec!["0", "30", "60"])
                            .bounds([0.0, 60.0]),
                    )
                    .y_axis(
                        Axis::default()
                            .title("Bytes/s)")
                            .labels(vec![fmt_bytes(0.0), fmt_bytes(ymax / 2.0), fmt_bytes(ymax)])
                            .bounds([0.0, ymax]),
                    )
                    .render(download_chunks[1], buf);
                }

                // Uploads and Upload Bandwidth
                {
                    let upload_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [Constraint::Percentage(30), Constraint::Percentage(70)].as_ref(),
                        )
                        .split(network_chunks[0]);

                    let uploads = List::new(state.blob_hashes.iter().map(|hash| {
                        let item = ListItem::new(hash.as_str());
                        item
                    }))
                    .block(
                        Block::default()
                            .title(format!("Blobs ({})", state.blob_hashes.len()))
                            .borders(Borders::ALL),
                    );

                    uploads.render(upload_chunks[0], buf);

                    // Placeholder for Upload Bandwidth
                    let upload_bandwidth = Paragraph::new("Upload Bandwidth Graph (Placeholder)")
                        .block(
                            Block::default()
                                .title("Upload Bandwidth")
                                .borders(Borders::ALL),
                        );
                    upload_bandwidth.render(upload_chunks[1], buf);
                }
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct UIDownloadProgress {
    downloaded: u64,
    total: u64,
}

#[derive(Default, Debug, Clone)]
pub struct NetworkTUIStateInner {
    pub join_ticket: PeerList,
    pub last_seen: HashMap<PublicKey, (ConnectionType, Instant)>,
    // pub data_per_sec_per_client: HashMap<PublicKey, f64>,
    pub total_data_per_sec: f64,
    pub download_bandwidth_history: VecDeque<f64>,

    pub downloads: HashMap<String, UIDownloadProgress>,

    pub blob_hashes: Vec<String>,
}

#[derive(Default, Debug, Clone)]
pub struct NetworkTUIState {
    pub inner: Option<NetworkTUIStateInner>,
}

impl<M, D> From<&NetworkConnection<M, D>> for NetworkTUIState
where
    M: Networkable,
    D: Networkable,
{
    fn from(nc: &NetworkConnection<M, D>) -> Self {
        let s = &nc.state;
        Self {
            inner: Some(NetworkTUIStateInner {
                join_ticket: s.join_ticket.clone(),
                last_seen: s.last_seen.clone(),
                total_data_per_sec: s.bandwidth_tracker.get_total_bandwidth(),
                download_bandwidth_history: s.bandwidth_history.clone(),
                downloads: s
                    .download_progesses
                    .iter()
                    .map(|(key, dl)| {
                        (
                            key.to_string(),
                            UIDownloadProgress {
                                downloaded: dl.downloaded_size,
                                total: dl.total_size,
                            },
                        )
                    })
                    .collect(),
                blob_hashes: s
                    .currently_sharing_blobs
                    .iter()
                    .map(|blob| blob.to_string())
                    .collect(),
            }),
        }
    }
}
