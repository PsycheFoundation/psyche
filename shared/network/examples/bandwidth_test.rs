use anyhow::{Result, bail};
use chrono::{Local, Timelike};
use clap::{ArgAction, Args, Parser, Subcommand};
use iroh::{PublicKey, RelayMode, RelayUrl};
use iroh_blobs::api::Tag;
use psyche_metrics::ClientMetrics;
use psyche_network::Hash;
use psyche_network::RelayKind;
use psyche_network::{
    BlobTicket, DiscoveryMode, DownloadType, NetworkConnection, NetworkEvent, NetworkTUIState,
    NetworkTui, allowlist, fmt_bytes,
};
use psyche_tui::{
    CustomWidget, LogOutput,
    logging::LoggerWidget,
    maybe_start_render_loop,
    ratatui::{
        layout::{Constraint, Direction, Layout},
        widgets::{Block, Borders, Paragraph, Widget},
    },
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::mpsc::Sender,
    time::{Interval, interval, interval_at},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    run_args: RunArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[derive(Args, Debug)]
struct RunArgs {
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

    connect_to: Option<String>,
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
                            error!("Network error: {err:#}{err:#}");
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
                network: NetworkTUIState::from_network_connection(&self.network)
                    .await
                    .unwrap(),
            };
            tx_tui_state.send(tui_state).await.unwrap();
        }
    }

    async fn on_network_event(&mut self, event: NetworkEvent<Message, DistroResultBlob>) {
        match event {
            NetworkEvent::MessageReceived((from, Message::Message { text })) => {
                info!(name:"message_recv_text", from=from.fmt_short().to_string(), text=text)
            }
            NetworkEvent::MessageReceived((from, Message::DistroResult { step, blob_ticket })) => {
                info!(name:"message_recv_distro", from=%from.fmt_short(), step=step, blob=%blob_ticket.hash().fmt_short());
                self.start_time.insert(blob_ticket.hash(), Instant::now());
                self.network.start_download(
                    blob_ticket,
                    Tag::from(step.to_string()),
                    DownloadType::DistroResult(Vec::new()),
                )
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
                );
                info!(name:"download_blob", from=%result.from.fmt_short(), step=file.step, blob=%hash.fmt_short());
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
            .expect("time went forward :)");
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
        rand::rng().fill(&mut data[..]);
        let endpoint_id = self.network.endpoint_id();

        let blob_ticket = match self
            .network
            .add_downloadable(DistroResultBlob { step, data }, Tag::from(step.to_string()))
            .await
        {
            Ok((v, _)) => {
                info!(name:"upload_blob", from=%endpoint_id.fmt_short(), step=step, blob=%v.hash().fmt_short());
                v
            }
            Err(err) => {
                error!("Couldn't add downloadable for step {step}. {err:#}");
                return;
            }
        };

        let message = Message::DistroResult {
            step,
            blob_ticket: blob_ticket.clone(),
        };

        if let Err(err) = self.network.broadcast(&message) {
            error!("Error sending message: {err}");
        } else {
            info!("broadcasted message for step {step}: {}", blob_ticket);
            info!(name:"message_send_distro", from=%endpoint_id.fmt_short(), step=step, blob=%blob_ticket.hash().fmt_short());
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if let Some(Commands::PrintAllHelp { markdown }) = cli_args.command {
        assert!(markdown);
        clap_markdown::print_help_markdown::<CliArgs>();
        return Ok(());
    }

    let args = cli_args.run_args;

    let logger = psyche_tui::logging()
        .with_output(if args.tui {
            LogOutput::TUI
        } else {
            LogOutput::Console
        })
        .init()?;

    let single_endpoint_id = args
        .connect_to
        .map(|p| {
            data_encoding::HEXLOWER
                .decode(p.as_bytes())
                .map(|b| PublicKey::try_from(&b as &[u8]))
                .expect("failed to parse endpoint id from arg")
        })
        .transpose()?
        .map(Into::into);

    info!("joining gossip room");

    let secret_key = args.secret_key.map(|k| k.parse().unwrap());

    let relay_mode = match (args.no_relay, args.relay) {
        (false, None) => RelayMode::Default,
        (false, Some(url)) => RelayMode::Custom(url.into()),
        (true, None) => RelayMode::Disabled,
        (true, Some(_)) => bail!("You cannot set --no-relay and --relay at the same time"),
    };
    info!("using relay servers: {:?}", &relay_mode);

    let tui = args.tui;
    let (cancel, tx_tui_state) = maybe_start_render_loop(tui.then(Tui::default))?;

    let network = NC::init(
        "123",
        args.bind_port,
        args.bind_interface,
        DiscoveryMode::N0,
        RelayKind::Psyche,
        single_endpoint_id.into_iter().collect(),
        secret_key,
        allowlist::AllowAll,
        Arc::new(ClientMetrics::new(None, None)),
        Some(cancel.clone()),
    )
    .await?;

    const SEND_DATA_INTERVAL: u64 = 3;
    // fire at wall-clock 3-second intervals.
    let send_data_interval = {
        let now = Local::now();
        let seconds_until_next: u64 =
            SEND_DATA_INTERVAL - (now.second() as u64 % SEND_DATA_INTERVAL);
        let start = Instant::now() + Duration::from_secs(seconds_until_next);
        interval_at(start.into(), Duration::from_secs(SEND_DATA_INTERVAL))
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

    logger.shutdown()?;

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
