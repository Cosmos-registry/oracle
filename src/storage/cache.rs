use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct BackoffCache {
    retries: HashMap<(String, u64), u8>,
}

impl BackoffCache {
    pub fn bump(&mut self, chain_id: &str, endpoint_id: u64) -> u8 {
        let key = (chain_id.to_string(), endpoint_id);
        let next = self.retries.get(&key).copied().unwrap_or(0).saturating_add(1);
        self.retries.insert(key, next);
        next
    }

    pub fn clear(&mut self, chain_id: &str, endpoint_id: u64) {
        self.retries.remove(&(chain_id.to_string(), endpoint_id));
    }
}
