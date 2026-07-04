use std::{collections::BTreeMap, sync::Arc, time::Duration};

use rand::Rng;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::{
    config::AppConfig,
    contract::queries::HttpContractSource,
    metrics::Metrics,
    models::{EndpointObservation, EndpointStatus, SubmissionBatch},
    publisher::Publisher,
    probes::ProbeEngine,
    storage::cache::BackoffCache,
};

pub struct Scheduler<P: Publisher> {
    cfg: AppConfig,
    source: HttpContractSource,
    probe_engine: ProbeEngine,
    publisher: P,
    metrics: Metrics,
    backoff: BackoffCache,
}

impl<P: Publisher> Scheduler<P> {
    pub fn new(
        cfg: AppConfig,
        source: HttpContractSource,
        probe_engine: ProbeEngine,
        publisher: P,
        metrics: Metrics,
    ) -> Self {
        Self {
            cfg,
            source,
            probe_engine,
            publisher,
            metrics,
            backoff: BackoffCache::default(),
        }
    }

    pub async fn run(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(self.cfg.oracle.probe_interval_secs));

        loop {
            ticker.tick().await;

            if let Err(e) = self.run_cycle().await {
                error!(error = %e, "cycle failed");
            }
        }
    }

    pub async fn run_cycle(&mut self) -> Result<(), crate::error::OracleError> {
        let now = now_seconds();
        let targets = self.source.fetch_targets().await?;
        let permit_pool = Arc::new(Semaphore::new(self.cfg.oracle.max_concurrency));

        let mut joins = Vec::with_capacity(targets.len());
        for target in targets {
            let jitter_delay = jitter_ms(self.cfg.oracle.jitter_pct);
            let permit_pool = Arc::clone(&permit_pool);
            let probe_engine = self.probe_engine.clone();

            joins.push(tokio::spawn(async move {
                let _permit = permit_pool.acquire_owned().await.ok();
                if jitter_delay > 0 {
                    tokio::time::sleep(Duration::from_millis(jitter_delay)).await;
                }
                probe_engine.probe(&target, now).await
            }));
        }

        let mut observations = Vec::new();
        for join in joins {
            self.metrics.probes_total.inc();
            match join.await {
                Ok(Ok(obs)) => {
                    self.metrics.probe_success_total.inc();
                    if let Some(latency) = obs.latency_ms {
                        let protocol = protocol_label(&obs);
                        self.metrics
                            .probe_latency_ms
                            .with_label_values(&[protocol])
                            .observe(latency as f64);
                    }
                    self.backoff.clear(&obs.chain_id, obs.endpoint_id);
                    observations.push(obs);
                }
                Ok(Err(e)) => {
                    self.metrics.probe_failure_total.inc();
                    warn!(error = %e, "probe error");
                }
                Err(e) => {
                    self.metrics.probe_failure_total.inc();
                    warn!(error = %e, "probe task join error");
                }
            }
        }

        self.publish_observations(observations).await;
        info!("cycle complete");
        Ok(())
    }

    async fn publish_observations(&mut self, observations: Vec<EndpointObservation>) {
        let mut by_chain: BTreeMap<String, Vec<EndpointObservation>> = BTreeMap::new();
        for obs in observations {
            by_chain.entry(obs.chain_id.clone()).or_default().push(obs);
        }

        for (chain_id, chain_observations) in by_chain {
            for chunk in chain_observations.chunks(self.cfg.oracle.batch_size) {
                let batch = SubmissionBatch {
                    chain_id: chain_id.clone(),
                    observations: chunk.to_vec(),
                };

                if let Err(e) = self.publisher.publish_batch(batch).await {
                    self.metrics.tx_failure_total.inc();
                    error!(chain_id = %chain_id, error = %e, "publish failed");
                } else {
                    self.metrics.tx_success_total.inc();
                }
            }
        }
    }
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn jitter_ms(jitter_pct: u8) -> u64 {
    if jitter_pct == 0 {
        return 0;
    }
    let mut rng = rand::thread_rng();
    let max_jitter = 1000_u64.saturating_mul(jitter_pct as u64) / 100;
    rng.gen_range(0..=max_jitter)
}

fn protocol_label(observation: &EndpointObservation) -> &'static str {
    use crate::models::EndpointType;
    match observation.endpoint_type {
        EndpointType::Rpc => "rpc",
        EndpointType::Rest => "rest",
        EndpointType::Grpc => "grpc",
        EndpointType::Websocket => "ws",
    }
}

#[allow(dead_code)]
fn _is_offline(obs: &EndpointObservation) -> bool {
    obs.status == EndpointStatus::Offline
}
