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
use tracing::{debug, error, trace, trace_span, warn, info, Instrument};

pub type BatchStep = u32;
pub type BatchIdSet = HashSet<BatchId>;

const MAX_RETRIES: u32 = 7;
const BASE_DELAY_MS: u64 = 2000;

pub struct DataFetcher<T: NodeIdentity, A: AuthenticatableIdentity> {
    data_providers: Vec<Arc<Mutex<DataProvider<A>>>>,
    active_fetch_task: Option<(BatchStep, JoinHandle<()>)>,
    buffer_size: usize,
    _phantom: PhantomData<T>,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> DataFetcher<T, A> {
    pub fn new(data_providers: Vec<DataProvider<A>>, buffer_size: usize) -> Self {
        Self {
            data_providers: data_providers
                .into_iter() // Use into_iter to consume the input vector
                .map(|dp| Arc::new(Mutex::new(dp))) // No need for clone here
                .collect(),
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
                let data_providers = self.data_providers.clone(); // Clone the Arc vector for the async task

                async move {
                    loop {
                        let batch_id = {
                            match assigned_batch_ids.pop() {
                                Some(assigned) => assigned,
                                None => {
                                    // out of assigned data!
                                    debug!("No more assigned batch IDs for step {step}.");
                                    return;
                                }
                            }
                        };

                        let mut batch_option = None;
                        for (provider_idx, data_provider) in data_providers.iter().enumerate() {
                            info!(batch_id = %batch_id, provider_idx, "Attempting fetch with provider {}", provider_idx);
                            let mut retry_count = 0;
                            loop {
                                match data_provider.lock().await.get_samples(batch_id).await {
                                    Ok(batch) => {
                                        info!(batch_id = %batch_id, provider_idx, "Successfully fetched batch with provider {}", provider_idx);
                                        batch_option = Some(batch);
                                        break; // Break retry loop, batch found
                                    },
                                    Err(err) if retry_count < MAX_RETRIES => {
                                        retry_count += 1;
                                        // Use exponential backoff with full jitter
                                        let delay_ms = BASE_DELAY_MS * 2u64.pow(retry_count - 1);
                                        let jitter = rand::random::<u64>() % delay_ms;
                                        let final_delay = Duration::from_millis(delay_ms / 2 + jitter); // Example: Full Jitter

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
                            }
                            if batch_option.is_some() {
                                break; // Break provider loop, batch found
                            }
                            // If batch_option is None here, it means the current provider failed permanently for this batch_id
                            warn!(batch_id = %batch_id, provider_idx, "Provider {} failed, trying next.", provider_idx);
                        }

                        // After trying all providers
                        let batch = match batch_option {
                            Some(b) => b,
                            None => {
                                error!(batch_id = %batch_id, "Failed to fetch batch after trying all data providers.");
                                // Decide how to handle this: skip the batch and continue, or stop the task?
                                // For now, let's skip this batch and try the next assigned ID.
                                continue; // Continue the outer loop to get the next batch_id
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
                    }
                }
                .instrument(trace_span!("fetch_data", step = step)) // Add step to span
            }),
        ));

        TrainingDataForStep { step, next_sample }
    }
}

pub struct TrainingDataForStep {
    pub step: u32,
    pub next_sample: mpsc::Receiver<Batch>,
}
