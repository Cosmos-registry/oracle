use tracing::warn;

use crate::{error::OracleError, models::SubmissionBatch, storage::queue::InMemoryQueue};

pub trait Publisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError>;
}

#[derive(Default)]
pub struct DegradedPublisher {
    pub queue: InMemoryQueue,
}

impl Publisher for DegradedPublisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError> {
        warn!(
            chain_id = %batch.chain_id,
            size = batch.observations.len(),
            "submit_endpoint_statuses unavailable on contract; queueing batch"
        );
        self.queue.push(batch);
        Ok(())
    }
}
