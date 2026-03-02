use psyche_coordinator::{Coordinator, get_batch_ids_for_node};
use psyche_core::{BatchId, NodeIdentity};
use psyche_data_provider::{DataProvider, TokenizedDataProvider};
use psyche_event_sourcing::event;
use psyche_modeling::{Batch, BatchData, BatchDataCPU};
use psyche_network::AuthenticatableIdentity;
use std::{
    collections::{BTreeMap, HashSet},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
    time::sleep,
};
use tracing::{Instrument, debug, error, trace, trace_span, warn};

pub type BatchStep = u32;
pub type BatchIdSet = HashSet<BatchId>;

const MAX_RETRIES: u32 = 7;
const BASE_DELAY_MS: u64 = 2000;

pub struct DataFetcher<T: NodeIdentity, A: AuthenticatableIdentity> {
    data_provider: Arc<Mutex<DataProvider<A>>>,
    active_fetch_task: Option<(BatchStep, JoinHandle<()>)>,
    buffer_size: usize,
    _phantom: PhantomData<T>,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> DataFetcher<T, A> {
    pub fn new(data_provider: DataProvider<A>, buffer_size: usize) -> Self {
        Self {
            data_provider: Arc::new(Mutex::new(data_provider)),
            active_fetch_task: None,
            buffer_size,
            _phantom: Default::default(),
        }
    }

    pub fn fetch_data(
        &mut self,
        state: &Coordinator<T>,
        data_assignments: &BTreeMap<BatchId, T>,
        identity: &T,
    ) -> TrainingDataForStep {
        let step = state.progress.step;

        let mut assigned_batch_ids = get_batch_ids_for_node(data_assignments, identity);
        trace!(
            name:"fetching_data_assignments",
            assigned_batch_ids = assigned_batch_ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
            "Fetching data assignments..."
        );

        let (tx_next_sample, next_sample) = mpsc::channel(self.buffer_size);

        if let Some((last_step, task)) = self.active_fetch_task.take() {
            trace!("Killing previous fetch task from step {last_step}.");
            task.abort(); // we don't need it anymore :)
        }

        self.active_fetch_task = Some((
            step,
            tokio::spawn({
                trace!("New fetch task for step {step} has been spawned");
                let data_provider = self.data_provider.clone(); // only one of these tasks will acquire the lock at once. once one dies, the lock is released for sure.

                async move {
                    loop {
                        let batch_id = {
                            match assigned_batch_ids.pop() {
                                Some(assigned) => assigned,
                                None => {
                                    // out of assigned data!
                                    return;
                                }
                            }
                        };

                        let mut retry_count = 0;
                        let batch = loop {
                            event!(train::BatchDataDownloadStart);
                            match data_provider.lock().await.get_samples(batch_id).await {
                                Ok(batch) => {
                                    event!(train::BatchDataDownloadComplete{result: Ok(())});
                                    break batch;
                                }
                                Err(err) if retry_count < MAX_RETRIES => {
                                    retry_count += 1;
                                    let delay_ms = BASE_DELAY_MS * (retry_count as u64 - 1);
                                    warn!(
                                        "Data fetch error for batch_id={} (attempt {}/{}): \"{:#}\". Retrying in {}ms",
                                        batch_id, retry_count, MAX_RETRIES, err, delay_ms
                                    );
                                    event!(train::BatchDataDownloadComplete{result: Err(())});

                                    sleep(Duration::from_millis(delay_ms)).await;
                                    continue;
                                }
                                Err(err) => {
                                    error!("Data fetch failed for batch_id={} after {} attempts: {err:#}", batch_id, MAX_RETRIES);
                                    event!(train::BatchDataDownloadComplete{result: Err(())});
                                    return;
                                }
                            }
                        };

                        if tx_next_sample
                            .send(Batch {
                                id: batch_id,
                                data: BatchData::CPU(batch.into_iter().map(|batch| {
                                    BatchDataCPU {
                                        input_ids: batch.input_ids,
                                        labels: batch.labels,
                                        position_ids: batch.position_ids,
                                        sequence_lengths: batch.sequence_lengths,
                                    }
                                }).collect()),
                            })
                            .await
                            .is_err()
                        {
                            debug!("Data loop finished");
                            return;
                        }
                    }
                }
                .instrument(trace_span!("fetch_data"))
            }),
        ));

        TrainingDataForStep { step, next_sample }
    }
}

pub struct TrainingDataForStep {
    pub step: u32,
    pub next_sample: mpsc::Receiver<Batch>,
}
