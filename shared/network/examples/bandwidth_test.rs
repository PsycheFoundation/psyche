use anyhow::{bail, Result};
use chrono::{Local, Timelike};
use clap::{ArgAction, Parser};
use iroh::{PublicKey, RelayMap, RelayMode, RelayUrl};
use psyche_network::Hash;
use psyche_network::{
    allowlist, fmt_bytes, BlobTicket, DiscoveryMode, NetworkConnection, NetworkEvent,
    NetworkTUIState, NetworkTui, PeerList,
};
use psyche_tui::{
    logging::LoggerWidget,
    maybe_start_render_loop,
    ratatui::{
        layout::{Constraint, Direction, Layout},
        widgets::{Block, Borders, Paragraph, Widget},
    },
    CustomWidget, LogOutput,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    str::FromStr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::mpsc::Sender,
    time::{interval, interval_at, Interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Level};

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,
    #[clap(short, long)]
    relay: Option<RelayUrl>,
    #[clap(long)]
    no_relay: bool,

    #[clap(short, long)]
    bind_port: Option<u16>,

    #[clap(long)]
    bind_interface: Option<String>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    tui: bool,

    peer_list: Option<String>,
}

type NC = NetworkConnection<Message, DistroResultBlob>;

#[derive(Default, Debug)]
struct TUIState {
    network: NetworkTUIState,
    current_step: u32,
}

#[derive(Default)]
struct Tui {
    network: NetworkTui,
}

impl CustomWidget for Tui {
    type Data = TUIState;

    fn render(
        &mut self,
        area: psyche_tui::ratatui::prelude::Rect,
        buf: &mut psyche_tui::ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // current step
                    Constraint::Max(3),
                    // network info
                    Constraint::Fill(1),
                    // console
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(area);
        Paragraph::new(format!("Current step: {}", state.current_step))
            .block(Block::new().borders(Borders::ALL))
            .render(chunks[0], buf);
        self.network.render(chunks[1], buf, &state.network);

        // console
        LoggerWidget::new().render(chunks[2], buf, &());
    }
}

#[derive(Debug)]
struct App {
    cancel: CancellationToken,
    current_step: u32,
    network: NC,
    tx_tui_state: Option<Sender<TUIState>>,
    send_data_interval: Interval,
    update_tui_interval: Interval,
    start_time: HashMap<Hash, Instant>,
}

impl App {
    async fn run(&mut self) {
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    break;
                }
                event = self.network.poll_next() => {
                    match event {
                        Ok(event) => {
                            if let Some(event) = event {
                                self.on_network_event(event).await;
                            }
                        }
                        Err(err) => {
                            error!("Network error: {err}");
                            return;
                        }
                    }
                }
                _ = self.send_data_interval.tick() => {
                    self.on_tick().await;
                }
                _ = self.update_tui_interval.tick() => {
                    self.update_tui().await;
                }
            }
        }
    }

    async fn update_tui(&mut self) {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let tui_state = TUIState {
                current_step: self.current_step,
                network: (&self.network).into(),
            };
            tx_tui_state.send(tui_state).await.unwrap();
        }
    }

    async fn on_network_event(&mut self, event: NetworkEvent<Message, DistroResultBlob>) {
        match event {
            NetworkEvent::MessageReceived((from, Message::Message { text })) => {
                info!("[{from}]: {text}")
            }
            NetworkEvent::MessageReceived((from, Message::DistroResult { step, blob_ticket })) => {
                info!("[{from}]: step {step} blob ticket {blob_ticket}");
                self.start_time.insert(blob_ticket.hash(), Instant::now());
                self.network
                    .start_download(blob_ticket, step)
                    .await
                    .unwrap();
            }
            NetworkEvent::DownloadComplete(result) => {
                let hash = result.hash;
                let file = result.data;
                let duration =
                    Instant::now() - self.start_time.remove(&hash).unwrap_or(Instant::now());
                let speed = file.data.len() as f64 / (duration.as_secs_f64() + 1e-6);
                info!(
                    "Download complete: {hash}! step {}: {} downloaded @ {}/s",
                    file.step,
                    fmt_bytes(file.data.len() as f64),
                    fmt_bytes(speed),
                )
            }
            NetworkEvent::DownloadFailed(result) => {
                info!(
                    "Download failed: {}! Reason: {}",
                    result.blob_ticket.hash(),
                    result.error
                )
            }
            _ => todo!(),
        }
    }
    async fn on_tick(&mut self) {
        let unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went forwads :)");
        let step = ((unix_time.as_secs() + 2) / 15) as u32;
        info!("new step {step}");
        if step != self.current_step + 1 {
            warn!(
                "new step {step} is not 1 greater than prev step {}",
                self.current_step + 1
            );
        }

        self.current_step = step;

        const DATA_SIZE_MB: usize = 10;
        let mut data = vec![0u8; DATA_SIZE_MB * 1024 * 1024];
        rand::thread_rng().fill(&mut data[..]);

        let blob_ticket = match self
            .network
            .add_downloadable(DistroResultBlob { step, data }, step)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Couldn't add downloadable for step {step}. {}", e);
                return;
            }
        };

        let message = Message::DistroResult {
            step,
            blob_ticket: blob_ticket.clone(),
        };

        if let Err(e) = self.network.broadcast(&message).await {
            error!("Error sending message: {}", e);
        } else {
            info!("broadcasted message for step {step}: {}", blob_ticket);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    psyche_tui::init_logging(
        if args.tui {
            LogOutput::TUI
        } else {
            LogOutput::Console
        },
        Level::INFO,
        None,
    );

    let PeerList(peers) = args
        .peer_list
        .map(|p| {
            PeerList::from_str(&p).unwrap_or_else(|_| {
                let single_node_id = data_encoding::HEXLOWER
                    .decode(p.as_bytes())
                    .map(|b| PublicKey::try_from(&b as &[u8]))
                    .expect("failed to parse peer list or node addr from arg")
                    .expect("failed to parse peer list or node addr from arg");
                PeerList(vec![single_node_id.into()])
            })
        })
        .unwrap_or_default();

    info!("joining gossip room");

    let secret_key = args.secret_key.map(|k| k.parse().unwrap());

    let relay_mode = match (args.no_relay, args.relay) {
        (false, None) => RelayMode::Default,
        (false, Some(url)) => RelayMode::Custom(RelayMap::from_url(url)),
        (true, None) => RelayMode::Disabled,
        (true, Some(_)) => bail!("You cannot set --no-relay and --relay at the same time"),
    };
    info!("using relay servers: {:?}", &relay_mode);

    let network = NC::init(
        "123",
        args.bind_port,
        args.bind_interface,
        relay_mode,
        DiscoveryMode::N0,
        peers,
        secret_key,
        allowlist::AllowAll,
        4,
    )
    .await?;

    let tui = args.tui;

    let (cancel, tx_tui_state) = maybe_start_render_loop(tui.then(Tui::default))?;

    // fire at wall-clock 15-second intervals.
    let send_data_interval = {
        let now = Local::now();
        let seconds_until_next: u64 = 15 - (now.second() as u64 % 15);
        let start = Instant::now() + Duration::from_secs(seconds_until_next);
        interval_at(start.into(), Duration::from_secs(15))
    };

    App {
        cancel,
        current_step: 0,
        network,
        tx_tui_state,
        send_data_interval,
        update_tui_interval: interval(Duration::from_millis(150)),
        start_time: HashMap::new(),
    }
    .run()
    .await;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    Message { text: String },
    DistroResult { blob_ticket: BlobTicket, step: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
struct DistroResultBlob {
    step: u32,
    data: Vec<u8>,
}
