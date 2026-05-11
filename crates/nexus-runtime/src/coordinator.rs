use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelTask {
    pub id: String,
    pub agent_id: String,
    pub description: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub agent_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub duration_ms: u64,
    pub tokens_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    WaitAll,
    FirstSuccess,
    Majority,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub success: bool,
    pub total_tasks: usize,
    pub completed: usize,
    pub failed: usize,
    pub merged_output: serde_json::Value,
    pub total_tokens: u64,
    pub wall_time_ms: u64,
    pub individual_results: Vec<TaskResult>,
}

#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, task: ParallelTask) -> TaskResult;
}

pub struct Coordinator {
    strategy: AggregationStrategy,
}

impl Coordinator {
    pub fn new(strategy: AggregationStrategy) -> Self {
        Self { strategy }
    }

    pub async fn execute(
        &self,
        tasks: Vec<ParallelTask>,
        executor: &dyn TaskExecutor,
    ) -> AggregatedResult {
        let total = tasks.len();
        let (tx, mut rx) = mpsc::channel::<TaskResult>(total.max(1));

        for task in tasks {
            let tx = tx.clone();
            let task_id = task.id.clone();
            let result = executor.execute(task).await;
            let _ = tx.send(result).await;
            info!(task_id = %task_id, "Task dispatched");
        }
        drop(tx);

        let mut results = Vec::new();
        while let Some(result) = rx.recv().await {
            results.push(result);
        }

        self.aggregate(results, total)
    }

    fn aggregate(&self, results: Vec<TaskResult>, total: usize) -> AggregatedResult {
        let successes = results.iter().filter(|r| r.success).count();
        let failures = results.iter().filter(|r| !r.success).count();

        let overall_success = match self.strategy {
            AggregationStrategy::WaitAll => failures == 0,
            AggregationStrategy::FirstSuccess => successes > 0,
            AggregationStrategy::Majority => successes > total / 2,
            AggregationStrategy::Merge => true,
        };

        let merged_output = match self.strategy {
            AggregationStrategy::Merge => {
                let mut merged = serde_json::Map::new();
                for r in &results {
                    if let serde_json::Value::Object(obj) = &r.output {
                        for (k, v) in obj {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                }
                serde_json::Value::Object(merged)
            }
            AggregationStrategy::FirstSuccess => results
                .iter()
                .find(|r| r.success)
                .map(|r| r.output.clone())
                .unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Array(results.iter().map(|r| r.output.clone()).collect()),
        };

        let total_tokens: u64 = results.iter().map(|r| r.tokens_used).sum();
        let wall_time_ms: u64 = results.iter().map(|r| r.duration_ms).max().unwrap_or(0);

        AggregatedResult {
            success: overall_success,
            total_tasks: total,
            completed: successes,
            failed: failures,
            merged_output,
            total_tokens,
            wall_time_ms,
            individual_results: results,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor {
        results: Vec<TaskResult>,
    }

    #[async_trait::async_trait]
    impl TaskExecutor for MockExecutor {
        async fn execute(&self, task: ParallelTask) -> TaskResult {
            self.results
                .iter()
                .find(|r| r.task_id == task.id)
                .cloned()
                .unwrap_or(TaskResult {
                    task_id: task.id,
                    agent_id: task.agent_id,
                    success: false,
                    output: serde_json::Value::Null,
                    duration_ms: 0,
                    tokens_used: 0,
                })
        }
    }

    fn make_task(id: &str) -> ParallelTask {
        ParallelTask {
            id: id.to_string(),
            agent_id: format!("agent-{id}"),
            description: format!("Task {id}"),
            payload: serde_json::json!({}),
        }
    }

    fn make_result(task_id: &str, success: bool, output: serde_json::Value) -> TaskResult {
        TaskResult {
            task_id: task_id.to_string(),
            agent_id: format!("agent-{task_id}"),
            success,
            output,
            duration_ms: 100,
            tokens_used: 500,
        }
    }

    #[tokio::test]
    async fn test_wait_all_all_succeed() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", true, serde_json::json!({"a": 1})),
                make_result("t2", true, serde_json::json!({"b": 2})),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::WaitAll);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert!(result.success);
        assert_eq!(result.completed, 2);
        assert_eq!(result.failed, 0);
        assert_eq!(result.total_tokens, 1000);
    }

    #[tokio::test]
    async fn test_wait_all_one_fails() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", true, serde_json::json!({"a": 1})),
                make_result("t2", false, serde_json::json!({"error": "boom"})),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::WaitAll);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert!(!result.success);
        assert_eq!(result.completed, 1);
        assert_eq!(result.failed, 1);
    }

    #[tokio::test]
    async fn test_first_success() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", false, serde_json::json!(null)),
                make_result("t2", true, serde_json::json!({"winner": true})),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::FirstSuccess);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert!(result.success);
        assert_eq!(result.merged_output, serde_json::json!({"winner": true}));
    }

    #[tokio::test]
    async fn test_first_success_all_fail() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", false, serde_json::json!(null)),
                make_result("t2", false, serde_json::json!(null)),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::FirstSuccess);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert!(!result.success);
        assert_eq!(result.merged_output, serde_json::Value::Null);
    }

    #[tokio::test]
    async fn test_majority_strategy() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", true, serde_json::json!(1)),
                make_result("t2", true, serde_json::json!(2)),
                make_result("t3", false, serde_json::json!(null)),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::Majority);
        let result = coord
            .execute(
                vec![make_task("t1"), make_task("t2"), make_task("t3")],
                &executor,
            )
            .await;

        assert!(result.success, "2/3 succeed = majority");
    }

    #[tokio::test]
    async fn test_majority_not_reached() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", true, serde_json::json!(1)),
                make_result("t2", false, serde_json::json!(null)),
                make_result("t3", false, serde_json::json!(null)),
                make_result("t4", false, serde_json::json!(null)),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::Majority);
        let result = coord
            .execute(
                vec![
                    make_task("t1"),
                    make_task("t2"),
                    make_task("t3"),
                    make_task("t4"),
                ],
                &executor,
            )
            .await;

        assert!(!result.success, "1/4 is not majority");
    }

    #[tokio::test]
    async fn test_merge_strategy() {
        let executor = MockExecutor {
            results: vec![
                make_result("t1", true, serde_json::json!({"files": ["a.rs"]})),
                make_result("t2", true, serde_json::json!({"tests": ["t1"]})),
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::Merge);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert!(result.success);
        let obj = result.merged_output.as_object().unwrap();
        assert!(obj.contains_key("files"));
        assert!(obj.contains_key("tests"));
    }

    #[tokio::test]
    async fn test_empty_tasks() {
        let executor = MockExecutor { results: vec![] };
        let coord = Coordinator::new(AggregationStrategy::WaitAll);
        let result = coord.execute(vec![], &executor).await;

        assert!(result.success);
        assert_eq!(result.total_tasks, 0);
    }

    #[tokio::test]
    async fn test_tokens_and_wall_time_aggregation() {
        let executor = MockExecutor {
            results: vec![
                TaskResult {
                    task_id: "t1".into(),
                    agent_id: "a1".into(),
                    success: true,
                    output: serde_json::json!(null),
                    duration_ms: 200,
                    tokens_used: 1000,
                },
                TaskResult {
                    task_id: "t2".into(),
                    agent_id: "a2".into(),
                    success: true,
                    output: serde_json::json!(null),
                    duration_ms: 500,
                    tokens_used: 2000,
                },
            ],
        };
        let coord = Coordinator::new(AggregationStrategy::WaitAll);
        let result = coord
            .execute(vec![make_task("t1"), make_task("t2")], &executor)
            .await;

        assert_eq!(result.total_tokens, 3000);
        assert_eq!(result.wall_time_ms, 500);
    }
}
