use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub agent_id: String,
    pub project_id: String,
    pub priority: u32,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub status: JobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
}

impl Eq for Job {}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Job {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap: "greatest" is popped first.
        // Lower priority number = higher urgency = pop first → reverse priority.
        // Earlier created_at = should pop first (FIFO within same priority) → reverse created_at.
        self.priority
            .cmp(&other.priority)
            .reverse()
            .then(other.created_at.cmp(&self.created_at))
    }
}

pub struct JobQueue {
    pending: Mutex<BinaryHeap<Job>>,
    active: Mutex<HashMap<String, Job>>,
    completed: Mutex<Vec<Job>>,
    notify: Notify,
    concurrency: usize,
}

impl JobQueue {
    pub fn new(concurrency: usize) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(BinaryHeap::new()),
            active: Mutex::new(HashMap::new()),
            completed: Mutex::new(Vec::new()),
            notify: Notify::new(),
            concurrency,
        })
    }

    pub async fn enqueue(&self, job: Job) {
        let mut pending = self.pending.lock().await;
        pending.push(job);
        self.notify.notify_one();
    }

    pub async fn dequeue(&self) -> Option<Job> {
        loop {
            {
                let active = self.active.lock().await;
                if active.len() >= self.concurrency {
                    drop(active);
                    self.notify.notified().await;
                    continue;
                }
            }
            let mut pending = self.pending.lock().await;
            if let Some(mut job) = pending.pop() {
                job.status = JobStatus::Running;
                let id = job.id.clone();
                let mut active = self.active.lock().await;
                active.insert(id, job.clone());
                return Some(job);
            }
            drop(pending);
            self.notify.notified().await;
        }
    }

    pub async fn complete(&self, job_id: &str, result: serde_json::Value) {
        let mut active = self.active.lock().await;
        if let Some(mut job) = active.remove(job_id) {
            job.status = JobStatus::Completed;
            job.result = Some(result);
            let mut completed = self.completed.lock().await;
            completed.push(job);
        }
        self.notify.notify_one();
    }

    pub async fn fail(&self, job_id: &str, retry: bool) {
        let mut active = self.active.lock().await;
        if let Some(mut job) = active.remove(job_id) {
            if retry && job.attempts < job.max_attempts {
                job.attempts += 1;
                job.status = JobStatus::Retrying;
                let mut pending = self.pending.lock().await;
                pending.push(job);
            } else {
                job.status = JobStatus::Failed;
                let mut completed = self.completed.lock().await;
                completed.push(job);
            }
        }
        self.notify.notify_one();
    }

    pub async fn stats(&self) -> QueueStats {
        let pending = self.pending.lock().await;
        let active = self.active.lock().await;
        let completed = self.completed.lock().await;
        QueueStats {
            pending: pending.len(),
            active: active.len(),
            completed: completed
                .iter()
                .filter(|j| j.status == JobStatus::Completed)
                .count(),
            failed: completed
                .iter()
                .filter(|j| j.status == JobStatus::Failed)
                .count(),
            concurrency: self.concurrency,
        }
    }

    /// Try to dequeue without blocking. Returns None if no jobs or at capacity.
    pub async fn try_dequeue(&self) -> Option<Job> {
        let active = self.active.lock().await;
        if active.len() >= self.concurrency {
            return None;
        }
        drop(active);

        let mut pending = self.pending.lock().await;
        if let Some(mut job) = pending.pop() {
            job.status = JobStatus::Running;
            let id = job.id.clone();
            let mut active = self.active.lock().await;
            active.insert(id, job.clone());
            Some(job)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: usize,
    pub active: usize,
    pub completed: usize,
    pub failed: usize,
    pub concurrency: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job(id: &str, priority: u32) -> Job {
        Job {
            id: id.to_string(),
            agent_id: "agent-1".into(),
            project_id: "project-1".into(),
            priority,
            payload: serde_json::json!({"task": id}),
            created_at: Utc::now(),
            status: JobStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            result: None,
        }
    }

    #[tokio::test]
    async fn test_enqueue_dequeue_fifo() {
        let queue = JobQueue::new(4);
        let j1 = make_job("j1", 5);
        let j2 = make_job("j2", 5);
        queue.enqueue(j1).await;
        queue.enqueue(j2).await;

        let first = queue.try_dequeue().await.unwrap();
        assert_eq!(first.id, "j1");
        let second = queue.try_dequeue().await.unwrap();
        assert_eq!(second.id, "j2");
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let queue = JobQueue::new(4);
        let low = make_job("low", 10);
        let high = make_job("high", 1);
        let mid = make_job("mid", 5);

        queue.enqueue(low).await;
        queue.enqueue(high).await;
        queue.enqueue(mid).await;

        let first = queue.try_dequeue().await.unwrap();
        assert_eq!(first.id, "high", "highest priority (lowest number) first");
        let second = queue.try_dequeue().await.unwrap();
        assert_eq!(second.id, "mid");
        let third = queue.try_dequeue().await.unwrap();
        assert_eq!(third.id, "low");
    }

    #[tokio::test]
    async fn test_concurrency_limit() {
        let queue = JobQueue::new(2);
        queue.enqueue(make_job("j1", 1)).await;
        queue.enqueue(make_job("j2", 1)).await;
        queue.enqueue(make_job("j3", 1)).await;

        let _a = queue.try_dequeue().await.unwrap();
        let _b = queue.try_dequeue().await.unwrap();
        let blocked = queue.try_dequeue().await;
        assert!(blocked.is_none(), "should block when at concurrency limit");
    }

    #[tokio::test]
    async fn test_complete_frees_slot() {
        let queue = JobQueue::new(1);
        queue.enqueue(make_job("j1", 1)).await;
        queue.enqueue(make_job("j2", 1)).await;

        let j1 = queue.try_dequeue().await.unwrap();
        assert!(queue.try_dequeue().await.is_none());

        queue.complete(&j1.id, serde_json::json!("done")).await;
        let j2 = queue.try_dequeue().await.unwrap();
        assert_eq!(j2.id, "j2");
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let queue = JobQueue::new(4);
        let mut job = make_job("retry-me", 1);
        job.max_attempts = 3;
        queue.enqueue(job).await;

        let j = queue.try_dequeue().await.unwrap();
        assert_eq!(j.attempts, 0);
        queue.fail(&j.id, true).await;

        let j2 = queue.try_dequeue().await.unwrap();
        assert_eq!(j2.id, "retry-me");
        assert_eq!(j2.attempts, 1);
        assert_eq!(j2.status, JobStatus::Running);
    }

    #[tokio::test]
    async fn test_max_retries_exhausted() {
        let queue = JobQueue::new(4);
        let mut job = make_job("fail-me", 1);
        job.max_attempts = 1;
        job.attempts = 1;
        queue.enqueue(job).await;

        let j = queue.try_dequeue().await.unwrap();
        queue.fail(&j.id, true).await;

        let stats = queue.stats().await;
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.pending, 0);
    }

    #[tokio::test]
    async fn test_stats() {
        let queue = JobQueue::new(4);
        queue.enqueue(make_job("j1", 1)).await;
        queue.enqueue(make_job("j2", 1)).await;

        let j1 = queue.try_dequeue().await.unwrap();
        queue.complete(&j1.id, serde_json::json!(null)).await;

        let stats = queue.stats().await;
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.concurrency, 4);
    }

    #[tokio::test]
    async fn test_fail_without_retry() {
        let queue = JobQueue::new(4);
        queue.enqueue(make_job("no-retry", 1)).await;

        let j = queue.try_dequeue().await.unwrap();
        queue.fail(&j.id, false).await;

        let stats = queue.stats().await;
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.pending, 0);
    }
}
