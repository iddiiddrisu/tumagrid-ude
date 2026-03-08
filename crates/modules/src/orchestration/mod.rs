/*!
 * API Orchestration Engine
 *
 * This module implements the core innovation of SpaceForge: the ability to compose
 * responses from multiple heterogeneous data sources in a single optimized request.
 *
 * COMPONENTS:
 * ===========
 * - QueryPlanner: Analyzes dependencies and creates parallel execution plans
 * - DataSourceRegistry: Manages different types of data sources
 * - ResponseComposer: Merges and transforms results into desired shape
 * - Executor: Runs the execution plan with proper error handling
 */

mod planner;
mod sources;
mod composer;
mod executor;

pub use planner::QueryPlanner;
pub use sources::{
    DataSourceRegistry, DataSourceExecutor,
    DatabaseExecutor, RestApiExecutor, GraphQLExecutor, FunctionExecutor, CacheExecutor,
};
pub use composer::ResponseComposer;
pub use executor::QueryExecutor;

// Re-export these from core for convenience
pub use ude_core::{DataSourceResult, ExecutionMetadata};
