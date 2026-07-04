use std::collections::VecDeque;

use crate::models::SubmissionBatch;

#[derive(Debug, Default)]
pub struct InMemoryQueue {
    queue: VecDeque<SubmissionBatch>,
}

impl InMemoryQueue {
    pub fn push(&mut self, batch: SubmissionBatch) {
        self.queue.push_back(batch);
    }

    pub fn pop(&mut self) -> Option<SubmissionBatch> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}
