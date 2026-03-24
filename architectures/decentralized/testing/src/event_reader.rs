use psyche_event_sourcing::events::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Read all events from all .postcard files in the given directory (non-recursive).
fn read_events_from_dir(dir: &Path) -> Vec<Event> {
    let mut events = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return events;
    };

    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "postcard")
        })
        .collect();

    // Sort by filename to get chronological order (epoch-N-timestamp)
    files.sort_by_key(|e| e.file_name());

    for entry in files {
        let mut file = fs::File::open(entry.path()).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();

        let mut cursor = 0;
        while cursor < data.len() {
            if let Some(event) =
                psyche_event_sourcing::store::try_decode_cobs_frame::<Event>(&data, &mut cursor)
            {
                events.push(event);
            } else {
                break;
            }
        }
    }

    events
}

/// Discover all node subdirectories under the events host dir and read their events.
/// Returns a list of `(node_id, events)` pairs.
pub fn read_all_node_events(events_host_dir: &Path) -> Vec<(String, Vec<Event>)> {
    let mut result = Vec::new();

    let Ok(entries) = fs::read_dir(events_host_dir) else {
        return result;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let node_id = entry.file_name().to_string_lossy().to_string();
            let events = read_events_from_dir(&path);
            if !events.is_empty() {
                result.push((node_id, events));
            }
        }
    }

    result
}

/// Get the path to the events host directory for the current test run.
pub fn events_host_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("EVENTS_HOST_DIR").unwrap_or_else(|_| "/tmp/psyche-test-events".to_string()),
    )
}

// ── Typed event extraction helpers ──────────────────────────────────────────

/// Extract all `Client::StateChanged` events.
pub fn state_changes(events: &[Event]) -> Vec<&client::StateChanged> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Client(Client::StateChanged(sc)) => Some(sc),
            _ => None,
        })
        .collect()
}

/// Extract all `Train::TrainingFinished` events.
pub fn training_finished(events: &[Event]) -> Vec<&train::TrainingFinished> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Train(Train::TrainingFinished(tf)) => Some(tf),
            _ => None,
        })
        .collect()
}

/// Extract loss values grouped by epoch from `TrainingFinished` events.
/// Returns Vec<(epoch, loss)> for events that have a non-NaN loss value.
pub fn losses_by_epoch(events: &[Event]) -> Vec<(u64, f64)> {
    training_finished(events)
        .into_iter()
        .filter_map(|tf| tf.loss.filter(|l| !l.is_nan()).map(|l| (tf.epoch, l)))
        .collect()
}

/// Extract the first loss per epoch (what the tests typically check for convergence).
/// Uses the first loss seen for each epoch, matching the old real-time test behavior
/// where the first `Response::Loss` for a new epoch was compared.
/// Skips NaN losses, matching the old test behavior.
pub fn first_loss_per_epoch(events: &[Event]) -> Vec<(u64, f64)> {
    let mut epoch_losses: std::collections::BTreeMap<u64, f64> = std::collections::BTreeMap::new();
    for (epoch, loss) in losses_by_epoch(events) {
        epoch_losses.entry(epoch).or_insert(loss);
    }
    epoch_losses.into_iter().collect()
}

/// Extract `Warmup::ModelLoadComplete` events.
pub fn model_load_complete(events: &[Event]) -> Vec<&warmup::ModelLoadComplete> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Warmup(Warmup::ModelLoadComplete(mlc)) => Some(mlc),
            _ => None,
        })
        .collect()
}

/// Extract `Client::HealthCheckFailed` events.
pub fn health_checks(events: &[Event]) -> Vec<&client::HealthCheckFailed> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Client(Client::HealthCheckFailed(hcf)) => Some(hcf),
            _ => None,
        })
        .collect()
}

/// Extract `Train::UntrainedBatchWarning` events.
pub fn untrained_batches(events: &[Event]) -> Vec<&train::UntrainedBatchWarning> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Train(Train::UntrainedBatchWarning(ubw)) => Some(ubw),
            _ => None,
        })
        .collect()
}

/// Extract `Train::WitnessElected` events where `is_witness` is true.
pub fn witness_elections(events: &[Event]) -> Vec<&train::WitnessElected> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Train(Train::WitnessElected(we)) if we.is_witness => Some(we),
            _ => None,
        })
        .collect()
}

/// Extract `CoordinatorEvent::SolanaSubscriptionChanged` events.
pub fn subscription_changes(
    events: &[Event],
) -> Vec<&coordinator::SolanaSubscriptionChanged> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::CoordinatorEvent(CoordinatorEvent::SolanaSubscriptionChanged(ssc)) => {
                Some(ssc)
            }
            _ => None,
        })
        .collect()
}

/// Extract `CoordinatorEvent::RpcFallback` events.
pub fn rpc_fallbacks(events: &[Event]) -> Vec<&coordinator::RpcFallback> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::CoordinatorEvent(CoordinatorEvent::RpcFallback(rf)) => Some(rf),
            _ => None,
        })
        .collect()
}

/// Extract `Client::Error` events.
pub fn errors(events: &[Event]) -> Vec<&client::Error> {
    events
        .iter()
        .filter_map(|e| match &e.data {
            EventData::Client(Client::Error(err)) => Some(err),
            _ => None,
        })
        .collect()
}
