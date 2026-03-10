use iroh::RelayMode;
use iroh_gossip::proto::TopicId;
use sha2::{Digest, Sha256};

const GOSSIP_TOPIC: &str = "psyche gossip";

pub fn gossip_topic(run_id: &str) -> TopicId {
    let mut hasher = Sha256::new();
    hasher.update(GOSSIP_TOPIC);
    hasher.update(run_id);
    let result = hasher.finalize();
    TopicId::from_bytes(result.into())
}

pub fn fmt_relay_mode(relay_mode: &RelayMode) -> String {
    match relay_mode {
        RelayMode::Disabled => "None".to_string(),
        RelayMode::Default => "Default iroh relay (production) servers".to_string(),
        RelayMode::Staging => "Default iroh relay (staging) servers".to_string(),
        RelayMode::Custom(map) => map
            .urls::<Vec<_>>()
            .into_iter()
            .map(|url| url.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

pub fn fmt_bytes(bytes: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    const PB: f64 = TB * 1024.0;

    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.2} KB", bytes / KB)
    } else if bytes < GB {
        format!("{:.2} MB", bytes / MB)
    } else if bytes < TB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes < PB {
        format!("{:.2} TB", bytes / TB)
    } else {
        format!("{:.2} PB", bytes / PB)
    }
}
