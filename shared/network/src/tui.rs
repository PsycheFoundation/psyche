use crate::{NetworkConnection, Networkable, P2PEndpointInfo, util::fmt_bytes};

use futures_util::StreamExt;
use iroh::EndpointId;
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
use std::collections::{HashMap, VecDeque};

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
                Paragraph::new(
                    state
                        .endpoint_id
                        .map(|m| m.to_string())
                        .unwrap_or("unknown".to_string()),
                )
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .title("Node ID")
                        .padding(Padding::symmetric(1, 0))
                        .borders(Borders::ALL),
                )
                .render(chunks[0], buf);

                List::new(state.endpoint_connections.iter().map(
                    |P2PEndpointInfo {
                         id: endpoint_id,
                         selected_path,
                         bandwidth,
                         latency,
                     }| {
                        let path_str = selected_path
                            .as_ref()
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let li = ListItem::new(format!(
                            "{} ({}): {} ({:.2}s)",
                            endpoint_id.fmt_short(),
                            path_str,
                            bandwidth,
                            latency,
                        ));
                        if *bandwidth > 1.0 && selected_path.is_some() {
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
    pub endpoint_id: Option<EndpointId>,
    pub endpoint_connections: Vec<P2PEndpointInfo>,
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

impl NetworkTUIState {
    pub async fn from_network_connection<M, D>(nc: &NetworkConnection<M, D>) -> anyhow::Result<Self>
    where
        M: Networkable,
        D: Networkable,
    {
        let s = &nc.state;
        let blob_hashes = nc
            .blobs_store
            .list()
            .stream()
            .await?
            .filter_map(|hash_result| async move { hash_result.ok().map(|h| h.to_string()) })
            .collect::<Vec<_>>()
            .await;

        Ok(Self {
            inner: Some(NetworkTUIStateInner {
                endpoint_id: s.endpoint_id,
                endpoint_connections: s.connection_info.clone(),
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
                blob_hashes,
            }),
        })
    }
}
