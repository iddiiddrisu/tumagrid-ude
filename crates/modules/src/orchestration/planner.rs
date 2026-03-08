/*!
 * Query Planner
 *
 * WHY THIS EXISTS:
 * ================
 * When a composite query has multiple data sources with dependencies, we need to:
 * 1. Determine the order of execution (topological sort of dependency graph)
 * 2. Identify which sources can run in parallel
 * 3. Group independent sources into execution stages
 * 4. Optimize the execution plan (merge requests, push filters, etc.)
 *
 * EXAMPLE:
 * ========
 * Query with sources:
 * - A (no dependencies)
 * - B (depends on A)
 * - C (depends on A)
 * - D (depends on B and C)
 *
 * Naive execution: A -> B -> C -> D (4 sequential steps)
 * Optimized plan:
 *   Stage 1: [A]           (1 query)
 *   Stage 2: [B, C]        (2 queries in parallel)
 *   Stage 3: [D]           (1 query)
 *
 * This reduces total execution time significantly!
 */

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use ude_core::*;
use std::collections::{HashMap, HashSet};

/// Query planner that creates optimized execution plans
///
/// WHY: Manual orchestration is error-prone and inefficient. The planner
/// automatically finds the optimal execution strategy.
pub struct QueryPlanner;

impl QueryPlanner {
    /// Create an execution plan from a composite query
    ///
    /// WHY: This is the core of the orchestration engine. It transforms a
    /// declarative query into an optimized execution plan that maximizes
    /// parallelization while respecting dependencies.
    pub fn plan(query: &CompositeQuery) -> Result<ExecutionPlan> {
        tracing::debug!(
            query_id = %query.id,
            num_sources = query.sources.len(),
            "Planning composite query execution"
        );

        // Build dependency graph
        let (graph, node_map) = Self::build_dependency_graph(&query.sources)?;

        // Validate graph (check for cycles)
        Self::validate_graph(&graph, &node_map)?;

        // Topological sort to get execution order
        let sorted = Self::topological_sort(&graph, &node_map)?;

        // Group into parallel stages
        let stages = Self::group_into_stages(&sorted, &graph, &node_map, &query.sources)?;

        // Estimate execution time
        let estimated_duration = Self::estimate_duration(&stages, &query.sources);

        tracing::info!(
            query_id = %query.id,
            num_stages = stages.len(),
            estimated_duration_ms = estimated_duration,
            "Created execution plan"
        );

        Ok(ExecutionPlan {
            stages,
            estimated_duration_ms: estimated_duration,
        })
    }

    /// Build a directed acyclic graph (DAG) of dependencies
    ///
    /// WHY: Graph representation makes it easy to analyze dependencies,
    /// detect cycles, and find parallel execution opportunities.
    fn build_dependency_graph(
        sources: &[DataSourceQuery],
    ) -> Result<(DiGraph<String, ()>, HashMap<String, NodeIndex>)> {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        // Create nodes
        for source in sources {
            let node = graph.add_node(source.id.clone());
            node_map.insert(source.id.clone(), node);
        }

        // Create edges for dependencies
        for source in sources {
            let target_node = node_map[&source.id];

            for dep_id in &source.depends_on {
                let source_node = node_map.get(dep_id).ok_or_else(|| Error::Validation {
                    field: "depends_on".to_string(),
                    message: format!(
                        "Source '{}' depends on unknown source '{}'",
                        source.id, dep_id
                    ),
                })?;

                // Edge from dependency to dependent
                graph.add_edge(*source_node, target_node, ());
            }
        }

        Ok((graph, node_map))
    }

    /// Validate the dependency graph
    ///
    /// WHY: Circular dependencies would cause infinite loops. We must detect
    /// and reject them early.
    fn validate_graph(
        graph: &DiGraph<String, ()>,
        node_map: &HashMap<String, NodeIndex>,
    ) -> Result<()> {
        // Check for cycles using topological sort
        if toposort(graph, None).is_err() {
            // Find a cycle for better error message
            let cycle = Self::find_cycle(graph, node_map);
            return Err(Error::Validation {
                field: "dependencies".to_string(),
                message: format!("Circular dependency detected: {}", cycle.join(" -> ")),
            });
        }

        Ok(())
    }

    /// Find a cycle in the graph (for error reporting)
    fn find_cycle(
        graph: &DiGraph<String, ()>,
        _node_map: &HashMap<String, NodeIndex>,
    ) -> Vec<String> {
        // Simple DFS to find cycle
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        for start_node in graph.node_indices() {
            if Self::dfs_find_cycle(graph, start_node, &mut visited, &mut path) {
                return path.iter().map(|&idx| graph[idx].clone()).collect();
            }
            visited.clear();
            path.clear();
        }

        vec!["Unknown cycle".to_string()]
    }

    fn dfs_find_cycle(
        graph: &DiGraph<String, ()>,
        node: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
        path: &mut Vec<NodeIndex>,
    ) -> bool {
        if path.contains(&node) {
            // Found cycle
            return true;
        }

        if visited.contains(&node) {
            return false;
        }

        visited.insert(node);
        path.push(node);

        for neighbor in graph.neighbors_directed(node, Direction::Outgoing) {
            if Self::dfs_find_cycle(graph, neighbor, visited, path) {
                return true;
            }
        }

        path.pop();
        false
    }

    /// Topological sort of the graph
    ///
    /// WHY: Topological sort gives us a valid execution order that respects
    /// all dependencies. If A depends on B, B will come before A in the sorted order.
    fn topological_sort(
        graph: &DiGraph<String, ()>,
        _node_map: &HashMap<String, NodeIndex>,
    ) -> Result<Vec<String>> {
        let sorted = toposort(graph, None).map_err(|_| {
            Error::Internal("Topological sort failed (graph has cycles)".to_string())
        })?;

        Ok(sorted.iter().map(|&idx| graph[idx].clone()).collect())
    }

    /// Group sources into parallel execution stages
    ///
    /// WHY: This is where the magic happens! Sources with no dependencies between
    /// them can run in parallel, dramatically reducing total execution time.
    ///
    /// ALGORITHM:
    /// 1. Start with sources that have no dependencies (Stage 1)
    /// 2. Remove them from the graph
    /// 3. Find sources whose dependencies are all satisfied (Stage 2)
    /// 4. Repeat until all sources are staged
    fn group_into_stages(
        sorted: &[String],
        _graph: &DiGraph<String, ()>,
        _node_map: &HashMap<String, NodeIndex>,
        sources: &[DataSourceQuery],
    ) -> Result<Vec<ExecutionStage>> {
        let mut stages = Vec::new();
        let mut completed = HashSet::new();
        let source_map: HashMap<_, _> = sources.iter().map(|s| (s.id.as_str(), s)).collect();

        while completed.len() < sorted.len() {
            let mut stage = Vec::new();

            // Find all sources whose dependencies are satisfied
            for source_id in sorted {
                if completed.contains(source_id) {
                    continue;
                }

                let source = source_map[source_id.as_str()];
                if source.depends_on.iter().all(|dep| completed.contains(dep)) {
                    stage.push(source_id.clone());
                }
            }

            if stage.is_empty() {
                return Err(Error::Internal(
                    "Could not create execution stages (possible bug in planner)".to_string(),
                ));
            }

            // Check if entire stage can be cached
            let cacheable = stage
                .iter()
                .all(|id| source_map[id.as_str()].cache.is_some());

            stages.push(ExecutionStage {
                queries: stage.clone(),
                cacheable,
            });

            completed.extend(stage);
        }

        tracing::debug!(
            num_stages = stages.len(),
            stage_sizes = ?stages.iter().map(|s| s.queries.len()).collect::<Vec<_>>(),
            "Grouped sources into execution stages"
        );

        Ok(stages)
    }

    /// Estimate execution duration
    ///
    /// WHY: Helps with timeout configuration and performance monitoring.
    /// Also useful for choosing between cached and fresh data.
    fn estimate_duration(stages: &[ExecutionStage], sources: &[DataSourceQuery]) -> u64 {
        let source_map: HashMap<_, _> = sources.iter().map(|s| (s.id.as_str(), s)).collect();

        stages
            .iter()
            .map(|stage| {
                // Max duration in stage (since queries run in parallel)
                stage
                    .queries
                    .iter()
                    .map(|id| {
                        source_map[id.as_str()].timeout_ms.unwrap_or(5000) // Default 5s
                    })
                    .max()
                    .unwrap_or(0)
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_simple_query() {
        let query = CompositeQuery {
            id: "test".to_string(),
            sources: vec![DataSourceQuery {
                id: "user".to_string(),
                source: DataSourceConfig::Database {
                    db_alias: "postgres".to_string(),
                    collection: "users".to_string(),
                    find: serde_json::json!({"id": 1}),
                    options: Default::default(),
                },
                depends_on: vec![],
                parallel: true,
                cache: None,
                timeout_ms: None,
                retry: None,
            }],
            compose: CompositionTemplate::Template(serde_json::json!({"user": "${user}"})),
            cache: None,
        };

        let plan = QueryPlanner::plan(&query).unwrap();
        assert_eq!(plan.stages.len(), 1);
        assert_eq!(plan.stages[0].queries.len(), 1);
    }

    #[test]
    fn test_plan_with_dependencies() {
        let query = CompositeQuery {
            id: "test".to_string(),
            sources: vec![
                DataSourceQuery {
                    id: "user".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "users".to_string(),
                        find: serde_json::json!({"id": 1}),
                        options: Default::default(),
                    },
                    depends_on: vec![],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
                DataSourceQuery {
                    id: "orders".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "orders".to_string(),
                        find: serde_json::json!({"user_id": "${user.id}"}),
                        options: Default::default(),
                    },
                    depends_on: vec!["user".to_string()],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
            ],
            compose: CompositionTemplate::Template(
                serde_json::json!({"user": "${user}", "orders": "${orders}"}),
            ),
            cache: None,
        };

        let plan = QueryPlanner::plan(&query).unwrap();
        assert_eq!(plan.stages.len(), 2);
        assert_eq!(plan.stages[0].queries, vec!["user"]);
        assert_eq!(plan.stages[1].queries, vec!["orders"]);
    }

    #[test]
    fn test_plan_parallel_execution() {
        let query = CompositeQuery {
            id: "test".to_string(),
            sources: vec![
                DataSourceQuery {
                    id: "user".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "users".to_string(),
                        find: serde_json::json!({"id": 1}),
                        options: Default::default(),
                    },
                    depends_on: vec![],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
                DataSourceQuery {
                    id: "orders".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "orders".to_string(),
                        find: serde_json::json!({"user_id": "${user.id}"}),
                        options: Default::default(),
                    },
                    depends_on: vec!["user".to_string()],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
                DataSourceQuery {
                    id: "recommendations".to_string(),
                    source: DataSourceConfig::Function {
                        name: "get_recs".to_string(),
                        args: std::collections::HashMap::new(),
                    },
                    depends_on: vec!["user".to_string()],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
            ],
            compose: CompositionTemplate::Template(serde_json::json!({})),
            cache: None,
        };

        let plan = QueryPlanner::plan(&query).unwrap();
        assert_eq!(plan.stages.len(), 2);
        assert_eq!(plan.stages[0].queries, vec!["user"]);
        // Stage 2 should have both orders and recommendations in parallel
        assert_eq!(plan.stages[1].queries.len(), 2);
        assert!(plan.stages[1].queries.contains(&"orders".to_string()));
        assert!(plan.stages[1]
            .queries
            .contains(&"recommendations".to_string()));
    }

    #[test]
    fn test_detect_circular_dependency() {
        let query = CompositeQuery {
            id: "test".to_string(),
            sources: vec![
                DataSourceQuery {
                    id: "a".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "users".to_string(),
                        find: serde_json::json!({}),
                        options: Default::default(),
                    },
                    depends_on: vec!["b".to_string()],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
                DataSourceQuery {
                    id: "b".to_string(),
                    source: DataSourceConfig::Database {
                        db_alias: "postgres".to_string(),
                        collection: "users".to_string(),
                        find: serde_json::json!({}),
                        options: Default::default(),
                    },
                    depends_on: vec!["a".to_string()],
                    parallel: true,
                    cache: None,
                    timeout_ms: None,
                    retry: None,
                },
            ],
            compose: CompositionTemplate::Template(serde_json::json!({})),
            cache: None,
        };

        let result = QueryPlanner::plan(&query);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circular dependency"));
    }
}
