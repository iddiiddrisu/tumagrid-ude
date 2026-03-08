/*!
 * Query Executor
 *
 * WHY THIS EXISTS:
 * ================
 * This is the orchestrator that brings everything together:
 * 1. Takes a CompositeQuery
 * 2. Uses QueryPlanner to create execution plan
 * 3. Executes each stage with proper parallelization
 * 4. Collects results from all data sources
 * 5. Uses ResponseComposer to create final response
 *
 * This is the "conductor" of the orchestra, coordinating all the pieces.
 */

use super::{DataSourceRegistry, QueryPlanner, ResponseComposer};
use ude_core::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Query executor that orchestrates composite queries
///
/// WHY: This is the main entry point for executing composite queries.
/// It handles the entire lifecycle from query planning to response composition.
pub struct QueryExecutor {
    _planner: QueryPlanner,
    registry: Arc<DataSourceRegistry>,
    composer: ResponseComposer,
}

impl QueryExecutor {
    pub fn new(registry: Arc<DataSourceRegistry>) -> Self {
        Self {
            _planner: QueryPlanner,
            registry,
            composer: ResponseComposer::new(),
        }
    }

    /// Execute a composite query
    ///
    /// WHY: This is the main API that clients call. It handles the entire
    /// orchestration process and returns the final composed response with metadata.
    ///
    /// FLOW:
    /// 1. Plan execution (analyze dependencies, create stages)
    /// 2. Execute each stage (stages run sequentially, queries within stages run in parallel)
    /// 3. Collect all results and metadata
    /// 4. Compose final response
    pub async fn execute(
        &self,
        ctx: &Context,
        query: &CompositeQuery,
    ) -> Result<QueryExecutionResult> {
        tracing::info!(
            query_id = %query.id,
            "Executing composite query"
        );

        // Plan execution
        let plan = QueryPlanner::plan(query)?;

        tracing::debug!(
            query_id = %query.id,
            num_stages = plan.stages.len(),
            "Execution plan created"
        );

        // Execute stages
        let results = self.execute_plan(ctx, query, &plan).await?;

        tracing::debug!(
            query_id = %query.id,
            num_results = results.len(),
            "All stages executed successfully"
        );

        // Collect execution metadata
        let mut used_cache = false;
        let mut warnings = Vec::new();

        for result in results.values() {
            if result.metadata.from_cache {
                used_cache = true;
            }
            warnings.extend(result.metadata.warnings.clone());
        }

        // Compose response
        let data = self.composer.compose(results, &query.compose)?;

        tracing::info!(
            query_id = %query.id,
            "Composite query executed successfully"
        );

        Ok(QueryExecutionResult {
            data,
            num_stages: plan.stages.len(),
            num_sources: query.sources.len(),
            used_cache,
            warnings,
        })
    }

    /// Execute the execution plan
    ///
    /// WHY: Stages must run sequentially (later stages depend on earlier ones),
    /// but queries within a stage can run in parallel. This maximizes throughput
    /// while respecting dependencies.
    async fn execute_plan(
        &self,
        ctx: &Context,
        query: &CompositeQuery,
        plan: &ExecutionPlan,
    ) -> Result<HashMap<String, DataSourceResult>> {
        let mut all_results = HashMap::new();
        let source_map: HashMap<_, _> = query.sources.iter().map(|s| (s.id.as_str(), s)).collect();

        // Execute each stage sequentially
        for (stage_idx, stage) in plan.stages.iter().enumerate() {
            tracing::debug!(
                stage = stage_idx + 1,
                total_stages = plan.stages.len(),
                num_queries = stage.queries.len(),
                "Executing stage"
            );

            // Execute queries in this stage in parallel
            let stage_results = self
                .execute_stage(ctx, stage, &source_map, &all_results)
                .await?;

            // Add stage results to accumulated results
            all_results.extend(stage_results);
        }

        Ok(all_results)
    }

    /// Execute a single stage (all queries in parallel)
    ///
    /// WHY: Parallel execution within a stage dramatically reduces total time.
    /// If a stage has 5 independent queries that each take 100ms, sequential
    /// execution takes 500ms, but parallel execution takes only 100ms!
    async fn execute_stage(
        &self,
        ctx: &Context,
        stage: &ExecutionStage,
        source_map: &HashMap<&str, &DataSourceQuery>,
        resolved_params: &HashMap<String, DataSourceResult>,
    ) -> Result<HashMap<String, DataSourceResult>> {
        let mut join_set = JoinSet::new();

        // Launch all queries in parallel
        for query_id in &stage.queries {
            let query = source_map[query_id.as_str()].clone();
            let registry = self.registry.clone();
            let ctx = ctx.clone();

            // Convert resolved results to params for template resolution
            let params: HashMap<String, serde_json::Value> = resolved_params
                .iter()
                .map(|(k, v)| (k.clone(), v.data.clone()))
                .collect();

            join_set.spawn(async move {
                Self::execute_single_query(&ctx, &query, &registry, &params).await
            });
        }

        // Collect results
        let mut results = HashMap::new();
        while let Some(result) = join_set.join_next().await {
            let query_result =
                result.map_err(|e| Error::Internal(format!("Task join error: {}", e)))??;

            results.insert(query_result.source_id.clone(), query_result);
        }

        Ok(results)
    }

    /// Execute a single data source query
    ///
    /// WHY: Each query type (database, REST API, function, etc.) needs different
    /// execution logic. The registry routes to the appropriate executor.
    async fn execute_single_query(
        ctx: &Context,
        query: &DataSourceQuery,
        registry: &DataSourceRegistry,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<DataSourceResult> {
        tracing::debug!(
            source_id = %query.id,
            source_type = ?query.source,
            "Executing data source query"
        );

        // Get appropriate executor
        let executor = registry.get_executor(&query.source)?;

        // Execute with timeout
        let timeout_duration =
            tokio::time::Duration::from_millis(query.timeout_ms.unwrap_or(30000));

        let result = tokio::time::timeout(
            timeout_duration,
            executor.execute(ctx, query, resolved_params),
        )
        .await
        .map_err(|_| Error::Timeout(timeout_duration))??;

        tracing::debug!(
            source_id = %query.id,
            duration_ms = result.metadata.duration_ms,
            from_cache = result.metadata.from_cache,
            "Data source query executed"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_simple_query() {
        // This test would require setting up mock executors
        // Skipping for now, but should be implemented when we have integration tests
    }
}
