use psyche_coordinator::{get_batch_ids_for_node, Coordinator};
use psyche_core::{BatchId, NodeIdentity};
use psyche_data_provider::{DataProvider, TokenizedDataProvider};
use psyche_modeling::{Batch, BatchData};
use psyche_network::AuthenticatableIdentity;
use std::{
    collections::{BTreeMap, HashSet},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
    time::sleep,
};
use tracing::{debug, error, info, trace, trace_span, warn, Instrument};

pub type BatchStep = u32;
pub type BatchIdSet = HashSet<BatchId>;

const MAX_RETRIES: u32 = 7;
const BASE_DELAY_MS: u64 = 2000;

pub struct DataFetcher<T: NodeIdentity, A: AuthenticatableIdentity> {
    data_providers: Vec<Arc<Mutex<DataProvider<A>>>>,
    active_fetch_task: Option<(BatchStep, JoinHandle<()>)>,
    buffer_size: usize,
    last_successful_provider_idx: Arc<Mutex<usize>>, // Store the index of the last successful provider
    _phantom: PhantomData<T>,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> DataFetcher<T, A> {
    pub fn new(data_providers: Vec<DataProvider<A>>, buffer_size: usize) -> Self {
        assert!(!data_providers.is_empty(), "Must provide at least one data provider");
        Self {
            data_providers: data_providers
                .into_iter()
                .map(|dp| Arc::new(Mutex::new(dp)))
                .collect(),
            active_fetch_task: None,
            buffer_size,
            last_successful_provider_idx: Arc::new(Mutex::new(0)), // Start with the first provider
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
                let data_providers = self.data_providers.clone();
                let last_successful_provider_idx = self.last_successful_provider_idx.clone(); // Clone Arc for the task

                async move {
                    let num_providers = data_providers.len();
                    if num_providers == 0 {
                        error!("No data providers configured.");
                        return;
                    }

                    loop {
                        let batch_id = {
                            match assigned_batch_ids.pop() {
                                Some(assigned) => assigned,
                                None => {
                                    debug!("No more assigned batch IDs for step {step}.");
                                    return;
                                }
                            }
                        };

                        let mut batch_option = None;
                        let start_idx = *last_successful_provider_idx.lock().await; // Read the last successful index

                        // Iterate through providers, starting from the last successful one and wrapping around
                        for i in 0..num_providers {
                            let provider_idx = (start_idx + i) % num_providers;
                            let data_provider = &data_providers[provider_idx];

                            info!(batch_id = %batch_id, provider_idx, "Attempting fetch with provider {}", provider_idx);
                            let mut retry_count = 0;
                            loop {
                                match data_provider.lock().await.get_samples(batch_id).await {
                                    Ok(batch) => {
                                        info!(batch_id = %batch_id, provider_idx, "Successfully fetched batch with provider {}", provider_idx);
                                        batch_option = Some(batch);
                                        // Update the last successful index
                                        *last_successful_provider_idx.lock().await = provider_idx;
                                        break; // Break retry loop, batch found
                                    },
                                    Err(err) if retry_count < MAX_RETRIES => {
                                        retry_count += 1;
                                        // Use exponential backoff with full jitter
                                        let delay_ms = BASE_DELAY_MS * 2u64.pow(retry_count - 1);
                                        let jitter = rand::random::<u64>() % delay_ms;
                                        let final_delay = Duration::from_millis(delay_ms / 2 + jitter);

                                        warn!(
                                            batch_id = %batch_id,
                                            provider_idx,
                                            attempt = retry_count,
                                            max_retries = MAX_RETRIES,
                                            error = %err,
                                            delay_ms = final_delay.as_millis(),
                                            "Data fetch error with provider {}. Retrying in {}ms",
                                            provider_idx, final_delay.as_millis()
                                        );
                                        sleep(final_delay).await;
                                        continue; // Continue retry loop
                                    }
                                    Err(err) => {
                                        error!(batch_id = %batch_id, provider_idx, error = %err, "Data fetch failed permanently for provider {}", provider_idx);
                                        break; // Break retry loop, provider failed permanently for this batch
                                    }
                                }
                            } // End retry loop

                            if batch_option.is_some() {
                                break; // Break provider loop (for i in 0..num_providers), batch found
                            }
                            // If batch_option is None here, it means the current provider failed permanently for this batch_id
                            warn!(batch_id = %batch_id, provider_idx, "Provider {} failed permanently for this batch, trying next.", provider_idx);
                        } // End provider loop

                        // After trying all providers
                        let batch = match batch_option {
                            Some(b) => b,
                            None => {
                                error!(batch_id = %batch_id, "Failed to fetch batch after trying all data providers.");
                                continue; // Skip this batch and try the next assigned ID
                            }
                        };

                        if tx_next_sample
                            .send(Batch {
                                id: batch_id,
                                data: BatchData::CPU(batch),
                            })
                            .await
                            .is_err()
                        {
                            debug!("Data loop finished because receiver dropped (step {step}).");
                            return; // Receiver is gone, stop the task
                        }
                    } // End main loop
                }
                .instrument(trace_span!("fetch_data", step = step))
            }),
        ));

        TrainingDataForStep { step, next_sample }
    }
}

pub struct TrainingDataForStep {
    pub step: u32,
    pub next_sample: mpsc::Receiver<Batch>,
}
