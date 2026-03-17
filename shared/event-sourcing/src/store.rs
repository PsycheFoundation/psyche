use crate::events::{Client, Event, EventData, RunStarted};
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use std::any::Any;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::mpsc::{self, UnboundedSender};
use tracing::error;

pub trait Backend: Send + Sync {
    fn emit(&self, event: Event);
    fn as_any(&self) -> &dyn Any;
}

#[derive(Default)]
pub struct InMemoryBackend {
    events: Mutex<Vec<Event>>,
    run_context: Mutex<Option<RunStarted>>,
}

impl InMemoryBackend {
    pub fn with_events<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Event]) -> R,
    {
        f(&self.events.lock())
    }

    pub fn push(&self, event: Event) {
        if let EventData::RunStarted(context) = &event.data {
            *self.run_context.lock() = Some(context.clone());
        }
        self.events.lock().push(event);
    }
}

impl Backend for InMemoryBackend {
    fn emit(&self, event: Event) {
        self.push(event);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct FileBackend {
    tx: UnboundedSender<Event>,
}

impl FileBackend {
    pub fn new(
        base_path: &Path,
        initial_epoch: u64,
        run_context: RunStarted,
        keep_event_files: Option<usize>,
    ) -> std::io::Result<Self> {
        if let Some(parent) = base_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
        let mut filewriter = FileWriterState::new(
            base_path.to_path_buf(),
            initial_epoch,
            run_context,
            keep_event_files,
        )?;

        tokio::runtime::Handle::try_current()
            .expect("FileBackend requires a tokio runtime")
            .spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if let EventData::Client(Client::StateChanged(ref sc)) = msg.data {
                        if sc.epoch > filewriter.current_epoch {
                            if let Err(e) = filewriter.write_event(&msg) {
                                error!("Failed to write StateChanged event: {}", e);
                            }
                            if let Err(e) = filewriter.rotate(sc.epoch) {
                                error!("Failed to rotate file: {}", e);
                            }
                            continue;
                        }
                    }
                    if let Err(e) = filewriter.write_event(&msg) {
                        error!("Failed to write event to disk: {}", e);
                    }
                }
            });

        Ok(Self { tx })
    }
}

impl Backend for FileBackend {
    fn emit(&self, event: Event) {
        if self.tx.send(event).is_err() {
            tracing::warn!("FileBackend event dropped: receiver task is gone");
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct EventStore {
    backends: Vec<Box<dyn Backend>>,
}

impl EventStore {
    fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    fn instance() -> &'static Arc<RwLock<EventStore>> {
        static INSTANCE: LazyLock<Arc<RwLock<EventStore>>> =
            LazyLock::new(|| Arc::new(RwLock::new(EventStore::new())));
        &INSTANCE
    }

    pub fn init(backends: Vec<Box<dyn Backend>>) {
        Self::instance().write().backends = backends;
    }

    pub fn emit(data: EventData, timestamp: DateTime<Utc>) {
        let store = Self::instance().read();
        let event = Event { timestamp, data };

        for backend in &store.backends {
            backend.emit(event.clone());
        }
    }

    pub fn with_backend<T: Backend + 'static, F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        let store = Self::instance().read();
        store
            .backends
            .iter()
            .find_map(|b| b.as_any().downcast_ref::<T>())
            .map(f)
    }

    /// Read COBS-framed events from disk into InMemoryBackend
    pub fn import_streamed_file<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        Self::with_backend::<InMemoryBackend, _, _>(|backend| {
            let mut cursor = 0;
            while cursor < data.len() {
                if let Some(event) = try_decode_cobs_frame::<Event>(&data, &mut cursor) {
                    backend.push(event);
                } else {
                    break;
                }
            }
        })
        .ok_or_else(|| std::io::Error::other("No InMemoryBackend configured"))
    }
}

pub(crate) fn try_decode_cobs_frame<T: serde::de::DeserializeOwned>(
    data: &[u8],
    cursor: &mut usize,
) -> Option<T> {
    while *cursor < data.len() {
        let remaining = &data[*cursor..];
        let Some(delimiter_pos) = remaining.iter().position(|&b| b == 0x00) else {
            // No more delimiters — remaining data is an incomplete frame (crash tail).
            *cursor = data.len();
            return None;
        };
        let frame = &remaining[..=delimiter_pos];

        match postcard::from_bytes_cobs::<T>(&mut frame.to_vec()) {
            Ok(decoded) => {
                *cursor += delimiter_pos + 1;
                return Some(decoded);
            }
            Err(_) => {
                // Corrupted frame — skip past this delimiter and try the next one.
                *cursor += delimiter_pos + 1;
            }
        }
    }
    None
}

struct FileWriterState {
    output_file: File,
    base_path: PathBuf,
    current_epoch: u64,
    run_context: RunStarted,
    keep_files: Option<usize>,
    created_files: VecDeque<PathBuf>,
}

impl FileWriterState {
    fn new(
        base_path: PathBuf,
        initial_epoch: u64,
        run_context: RunStarted,
        keep_files: Option<usize>,
    ) -> std::io::Result<Self> {
        let mut created_files = VecDeque::new();
        let output_file = Self::open_new_file(
            &base_path,
            initial_epoch,
            run_context.clone(),
            &mut created_files,
        )?;
        Ok(Self {
            output_file,
            base_path,
            current_epoch: initial_epoch,
            run_context,
            keep_files,
            created_files,
        })
    }

    fn open_new_file(
        base_path: &Path,
        epoch: u64,
        run_context: RunStarted,
        created_files: &mut VecDeque<PathBuf>,
    ) -> std::io::Result<File> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
        let filename = format!("events-epoch-{}-{}.postcard", epoch, timestamp);
        let file_path = base_path.join(&filename);

        std::fs::create_dir_all(base_path)?;

        let mut file = File::create(&file_path)?;

        // each file is self-contained with context
        let run_started = Event {
            timestamp: Utc::now(),
            data: EventData::RunStarted(run_context),
        };
        file.write_all(&postcard::to_stdvec_cobs(&run_started).map_err(std::io::Error::other)?)?;

        created_files.push_back(file_path);

        Ok(file)
    }

    fn write_event(&mut self, event: &Event) -> std::io::Result<()> {
        let data = postcard::to_stdvec_cobs(event).map_err(std::io::Error::other)?;
        self.output_file.write_all(&data)
    }

    fn rotate(&mut self, new_epoch: u64) -> std::io::Result<()> {
        self.current_epoch = new_epoch;
        self.output_file = Self::open_new_file(
            &self.base_path,
            self.current_epoch,
            self.run_context.clone(),
            &mut self.created_files,
        )?;
        while self
            .keep_files
            .is_some_and(|keep| self.created_files.len() > keep)
        {
            let old = self.created_files.pop_front().unwrap();
            if let Err(e) = std::fs::remove_file(&old) {
                error!("Failed to delete old events file {}: {}", old.display(), e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event, events::*};
    use psyche_coordinator::RunState;
    use psyche_core::{BatchId, ClosedInterval};
    use serial_test::serial;
    use std::fs;
    use std::sync::LazyLock;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;

    static TEST_RUNTIME: LazyLock<Runtime> =
        LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

    fn test_run_context() -> RunStarted {
        RunStarted {
            run_id: "test-run-123".into(),
            node_id: "node-1".into(),
            config: "abc123".into(),
            psyche_version: "0.1.0".into(),
        }
    }

    fn test_batch_id() -> BatchId {
        BatchId(ClosedInterval::new(1, 1))
    }

    #[test]
    #[serial]
    fn test_inmemory_backend_basic() {
        EventStore::init(vec![Box::new(InMemoryBackend::default())]);

        event!(test_run_context());

        event!(ResourceSnapshot {
            gpu_mem_used_bytes: Some(1024),
            gpu_utilization_percent: Some(75.5),
            cpu_mem_used_bytes: 2048,
            cpu_utilization_percent: 50.0,
            network_bytes_sent_total: 1000,
            network_bytes_recv_total: 2000,
            disk_space_available_bytes: 10000,
        });

        let count =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.len()));
        assert_eq!(count, Some(2));
    }

    #[test]
    #[serial]
    fn test_multiple_backends() {
        let _guard = TEST_RUNTIME.enter();
        let temp_dir = TempDir::new().unwrap();

        EventStore::init(vec![
            Box::new(InMemoryBackend::default()),
            Box::new(FileBackend::new(temp_dir.path(), 0, test_run_context(), Some(5)).unwrap()),
        ]);

        event!(train::TrainingStarted {
            batch_id: test_batch_id()
        });

        event!(train::TrainingFinished {
            batch_id: test_batch_id(),
            step: 1,
            loss: Some(0.5),
        });

        std::thread::sleep(std::time::Duration::from_millis(100));

        let count =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.len()));
        assert_eq!(count, Some(2));

        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    #[serial]
    fn test_file_backend_rotation() {
        let _guard = TEST_RUNTIME.enter();
        let temp_dir = TempDir::new().unwrap();

        EventStore::init(vec![Box::new(
            FileBackend::new(temp_dir.path(), 0, test_run_context(), Some(5)).unwrap(),
        )]);

        event!(train::TrainingStarted {
            batch_id: test_batch_id()
        });

        event!(client::StateChanged {
            old_state: RunState::RoundTrain,
            new_state: RunState::Cooldown,
            epoch: 1,
            step: 10,
        });

        event!(train::TrainingFinished {
            batch_id: test_batch_id(),
            step: 10,
            loss: None,
        });

        std::thread::sleep(std::time::Duration::from_millis(200));

        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 2);

        let filenames: Vec<String> = files
            .iter()
            .map(|f| f.file_name().to_string_lossy().to_string())
            .collect();
        assert!(filenames.iter().any(|f| f.contains("epoch-0")));
        assert!(filenames.iter().any(|f| f.contains("epoch-1")));
    }

    #[test]
    #[serial]
    fn test_import_streamed_file() {
        let _guard = TEST_RUNTIME.enter();
        let temp_dir = TempDir::new().unwrap();

        EventStore::init(vec![Box::new(
            FileBackend::new(temp_dir.path(), 0, test_run_context(), Some(5)).unwrap(),
        )]);

        let batch_id = test_batch_id();

        event!(train::TrainingStarted { batch_id });

        event!(train::TrainingFinished {
            batch_id,
            step: 42,
            loss: Some(0.123),
        });

        std::thread::sleep(std::time::Duration::from_millis(100));

        let file_path = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap()
            .path();

        EventStore::init(vec![Box::new(InMemoryBackend::default())]);
        EventStore::import_streamed_file(&file_path).unwrap();

        let count =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.len()));
        assert_eq!(count, Some(3));

        let events =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.to_vec()))
                .unwrap();
        assert!(matches!(events[0].data, EventData::RunStarted(_)));
        assert!(matches!(
            events[1].data,
            EventData::Train(Train::TrainingStarted(_))
        ));
        assert!(matches!(
            events[2].data,
            EventData::Train(Train::TrainingFinished(_))
        ));
    }

    #[test]
    #[serial]
    fn test_event_data_in_variants() {
        EventStore::init(vec![Box::new(InMemoryBackend::default())]);

        let blob = iroh_blobs::Hash::from_bytes([1u8; 32]);
        event!(p2p::BlobDownloadStarted {
            blob,
            size_bytes: 1024,
        });

        let events =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.to_vec()))
                .unwrap();
        match &events[0].data {
            EventData::P2P(P2P::BlobDownloadStarted(bds)) => {
                assert_eq!(bds.blob, blob);
                assert_eq!(bds.size_bytes, 1024);
            }
            other => panic!("unexpected event: {:?}", other),
        }
    }

    #[test]
    #[serial]
    fn test_file_backend_retention() {
        let _guard = TEST_RUNTIME.enter();
        let temp_dir = TempDir::new().unwrap();

        EventStore::init(vec![Box::new(
            FileBackend::new(temp_dir.path(), 0, test_run_context(), Some(2)).unwrap(),
        )]);

        // epoch 0, 1, 2, 3: should keep only 2,3
        for epoch in 1u64..=3 {
            event!(client::StateChanged {
                old_state: RunState::RoundTrain,
                new_state: RunState::Cooldown,
                epoch,
                step: epoch * 10,
            });
        }

        std::thread::sleep(std::time::Duration::from_millis(200));

        let mut files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        files.sort();

        assert_eq!(files.len(), 2, "expected 2 files, got: {:?}", files);
        assert!(files.iter().any(|f| f.contains("epoch-2")));
        assert!(files.iter().any(|f| f.contains("epoch-3")));
    }

    #[test]
    #[serial]
    fn test_no_backends_initialized() {
        EventStore::init(vec![]);

        event!(train::TrainingStarted {
            batch_id: test_batch_id()
        });

        let count =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.len()));
        assert_eq!(count, None);
    }

    #[test]
    #[serial]
    fn test_file_only_backend() {
        let _guard = TEST_RUNTIME.enter();
        let temp_dir = TempDir::new().unwrap();

        EventStore::init(vec![Box::new(
            FileBackend::new(temp_dir.path(), 0, test_run_context(), Some(5)).unwrap(),
        )]);

        event!(train::TrainingStarted {
            batch_id: test_batch_id()
        });

        let count =
            EventStore::with_backend::<InMemoryBackend, _, _>(|b| b.with_events(|e| e.len()));
        assert_eq!(count, None);

        std::thread::sleep(std::time::Duration::from_millis(100));
        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
    }
}
