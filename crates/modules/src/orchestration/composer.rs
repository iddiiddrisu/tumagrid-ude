/*!
 * Response Composer
 *
 * WHY THIS EXISTS:
 * ================
 * After fetching data from multiple sources, we need to:
 * 1. Merge results from different sources
 * 2. Transform data to match client's desired shape
 * 3. Perform cross-source joins (like SQL JOINs but across APIs!)
 * 4. Apply filters and projections
 * 5. Handle template expressions
 *
 * This is the final step that takes raw results and produces the perfect
 * response that the client requested.
 *
 * EXAMPLE:
 * ========
 * Input: {
 *   user: [{id: 1, name: "John"}],
 *   orders: [{id: 10, user_id: 1, total: 100}],
 *   shipping: [{order_id: 10, status: "shipped"}]
 * }
 *
 * Composition Template: {
 *   user: "${user[0]}",
 *   orders: "${orders | map(o => merge(o, shipping.find(s => s.order_id == o.id)))}"
 * }
 *
 * Output: {
 *   user: {id: 1, name: "John"},
 *   orders: [{id: 10, user_id: 1, total: 100, status: "shipped"}]
 * }
 */

use ude_core::*;
use std::collections::HashMap;
use tera::Tera;

/// Response composer that transforms and merges data from multiple sources
///
/// WHY: The client doesn't want separate results from each source - they want
/// a single, perfectly shaped response. This composer handles all the complex
/// data transformations.
pub struct ResponseComposer {
    _template_engine: Tera,
}

impl ResponseComposer {
    pub fn new() -> Self {
        Self {
            _template_engine: Tera::default(),
        }
    }

    /// Compose the final response from multiple data source results
    ///
    /// WHY: This is where all the magic happens - taking disparate data from
    /// databases, APIs, functions, etc. and combining it into exactly what the
    /// client needs.
    pub fn compose(
        &self,
        results: HashMap<String, DataSourceResult>,
        template: &CompositionTemplate,
    ) -> Result<serde_json::Value> {
        tracing::debug!(
            num_results = results.len(),
            "Composing response from data sources"
        );

        match template {
            CompositionTemplate::Template(tmpl) => self.compose_simple(results, tmpl),
            CompositionTemplate::Advanced { fields, filters } => {
                self.compose_advanced(results, fields, filters.as_ref())
            }
        }
    }

    /// Simple composition using template substitution
    ///
    /// WHY: For simple cases, just replacing "${source.field}" with actual data
    /// is sufficient and more performant than complex transformations.
    fn compose_simple(
        &self,
        results: HashMap<String, DataSourceResult>,
        template: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        match template {
            Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
                // Resolve template variable: "${user.name}"
                let path = &s[2..s.len() - 1];
                self.resolve_path(path, &results)
            }
            Value::Object(map) => {
                let mut composed = serde_json::Map::new();
                for (key, value) in map {
                    composed.insert(key.clone(), self.compose_simple(results.clone(), value)?);
                }
                Ok(Value::Object(composed))
            }
            Value::Array(arr) => {
                let composed: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| self.compose_simple(results.clone(), v))
                    .collect();
                Ok(Value::Array(composed?))
            }
            _ => Ok(template.clone()),
        }
    }

    /// Advanced composition with transformations
    ///
    /// WHY: Complex use cases need more than simple substitution - need joins,
    /// maps, filters, and other transformations.
    fn compose_advanced(
        &self,
        results: HashMap<String, DataSourceResult>,
        fields: &HashMap<String, FieldTransform>,
        filters: Option<&Vec<Filter>>,
    ) -> Result<serde_json::Value> {
        let mut composed = serde_json::Map::new();

        // Apply transformations to each field
        for (field_name, transform) in fields {
            let value = self.apply_transform(transform, &results)?;
            composed.insert(field_name.clone(), value);
        }

        let mut result = serde_json::Value::Object(composed);

        // Apply filters if present
        if let Some(filter_list) = filters {
            for filter in filter_list {
                result = self.apply_filter(&result, filter)?;
            }
        }

        Ok(result)
    }

    /// Apply a field transformation
    ///
    /// WHY: Different transformations serve different purposes:
    /// - Reference: Simple "${user.name}" substitution
    /// - Map: Transform arrays "${orders | map(o => o.total)}"
    /// - Merge: Combine objects from different sources
    /// - Filter: Select subset "${orders | filter(o => o.status == 'active')}"
    /// - Join: Cross-source joins "${orders JOIN shipping ON order_id}"
    fn apply_transform(
        &self,
        transform: &FieldTransform,
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        match transform {
            FieldTransform::Reference(path) => self.resolve_path(path, results),

            FieldTransform::Map { source, transform } => {
                self.apply_map_transform(source, transform, results)
            }

            FieldTransform::Merge { sources } => self.merge_sources(sources, results),

            FieldTransform::Filter { source, condition } => {
                self.apply_filter_transform(source, condition, results)
            }

            FieldTransform::Join {
                left,
                right,
                left_key,
                right_key,
                join_type,
            } => self.join_sources(left, right, left_key, right_key, join_type, results),
        }
    }

    /// Resolve a path like "user.name" or "orders[0].id"
    ///
    /// WHY: Template expressions reference data from sources using paths.
    /// This resolves those paths to actual values.
    fn resolve_path(
        &self,
        path: &str,
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        // Remove template markers if present
        let path = path.trim_start_matches("${").trim_end_matches('}');

        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return Err(Error::Validation {
                field: "path".to_string(),
                message: "Empty path".to_string(),
            });
        }

        // First part is the source ID
        let source_id = parts[0];
        let result = results.get(source_id).ok_or_else(|| Error::Validation {
            field: "source".to_string(),
            message: format!("Source '{}' not found", source_id),
        })?;

        // Navigate through the rest of the path
        let mut current = &result.data;
        for part in &parts[1..] {
            // Handle array indexing: "orders[0]"
            if let Some(idx_start) = part.find('[') {
                let field = &part[..idx_start];
                let idx_str = &part[idx_start + 1..part.len() - 1];
                let idx: usize = idx_str.parse().map_err(|_| Error::Validation {
                    field: "index".to_string(),
                    message: format!("Invalid array index: {}", idx_str),
                })?;

                current = current.get(field).and_then(|v| v.get(idx)).ok_or_else(|| {
                    Error::Validation {
                        field: "path".to_string(),
                        message: format!("Path '{}' not found", path),
                    }
                })?;
            } else {
                current = current.get(part).ok_or_else(|| Error::Validation {
                    field: "path".to_string(),
                    message: format!("Path '{}' not found", path),
                })?;
            }
        }

        Ok(current.clone())
    }

    /// Merge data from multiple sources
    ///
    /// WHY: Often need to combine data from different sources into a single object.
    /// Example: Merge user profile from DB with preferences from API.
    fn merge_sources(
        &self,
        sources: &[String],
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        use serde_json::{Map, Value};

        let mut merged = Map::new();

        for source_id in sources {
            let result = results.get(source_id).ok_or_else(|| Error::Validation {
                field: "source".to_string(),
                message: format!("Source '{}' not found", source_id),
            })?;

            if let Value::Object(obj) = &result.data {
                for (k, v) in obj {
                    merged.insert(k.clone(), v.clone());
                }
            } else if let Value::Array(arr) = &result.data {
                if let Some(Value::Object(obj)) = arr.first() {
                    for (k, v) in obj {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        Ok(Value::Object(merged))
    }

    /// Join two data sources (like SQL JOIN)
    ///
    /// WHY: This is THE killer feature! Join data from completely different sources:
    /// - Join orders from MongoDB with shipping status from REST API
    /// - Join users from Postgres with preferences from GraphQL service
    /// - Any combination imaginable!
    ///
    /// This is what sets SpaceForge apart from other tools.
    fn join_sources(
        &self,
        left: &str,
        right: &str,
        left_key: &str,
        right_key: &str,
        join_type: &JoinType,
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        tracing::debug!(
            left = left,
            right = right,
            left_key = left_key,
            right_key = right_key,
            join_type = ?join_type,
            "Performing cross-source join"
        );

        // Get left and right data
        let left_result = results.get(left).ok_or_else(|| Error::Validation {
            field: "left".to_string(),
            message: format!("Source '{}' not found", left),
        })?;

        let right_result = results.get(right).ok_or_else(|| Error::Validation {
            field: "right".to_string(),
            message: format!("Source '{}' not found", right),
        })?;

        // Convert right data to hashmap for efficient lookup
        let mut right_map: HashMap<String, &Value> = HashMap::new();
        if let Value::Array(arr) = &right_result.data {
            for item in arr {
                if let Value::Object(obj) = item {
                    if let Some(Value::String(key)) = obj.get(right_key) {
                        right_map.insert(key.clone(), item);
                    }
                }
            }
        }

        // Perform join
        let mut joined = Vec::new();

        if let Value::Array(left_arr) = &left_result.data {
            for left_item in left_arr {
                if let Value::Object(left_obj) = left_item {
                    if let Some(Value::String(key_value)) = left_obj.get(left_key) {
                        match join_type {
                            JoinType::Inner => {
                                if let Some(right_item) = right_map.get(key_value) {
                                    let merged = self.merge_objects(left_obj, right_item)?;
                                    joined.push(merged);
                                }
                            }
                            JoinType::Left => {
                                if let Some(right_item) = right_map.get(key_value) {
                                    let merged = self.merge_objects(left_obj, right_item)?;
                                    joined.push(merged);
                                } else {
                                    // Left join: include left item even without match
                                    joined.push(Value::Object(left_obj.clone()));
                                }
                            }
                            JoinType::Right => {
                                // Right join: all items from right, with matching left items
                                // This is handled after the loop
                                if let Some(right_item) = right_map.get(key_value) {
                                    let merged = self.merge_objects(left_obj, right_item)?;
                                    joined.push(merged);
                                }
                            }
                            JoinType::Outer => {
                                // Full outer join: all items from both sides
                                if let Some(right_item) = right_map.get(key_value) {
                                    let merged = self.merge_objects(left_obj, right_item)?;
                                    joined.push(merged);
                                } else {
                                    // No match on right, include left item
                                    joined.push(Value::Object(left_obj.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        // For Right and Outer joins, add unmatched right items
        if matches!(join_type, JoinType::Right | JoinType::Outer) {
            // Track which right items were matched
            let mut matched_right_keys = std::collections::HashSet::new();

            if let Value::Array(left_arr) = &left_result.data {
                for left_item in left_arr {
                    if let Value::Object(left_obj) = left_item {
                        if let Some(Value::String(key_value)) = left_obj.get(left_key) {
                            matched_right_keys.insert(key_value.clone());
                        }
                    }
                }
            }

            // Add unmatched right items
            if let Value::Array(arr) = &right_result.data {
                for item in arr {
                    if let Value::Object(obj) = item {
                        if let Some(Value::String(key)) = obj.get(right_key) {
                            if !matched_right_keys.contains(key) {
                                joined.push(item.clone());
                            }
                        }
                    }
                }
            }
        }

        Ok(Value::Array(joined))
    }

    fn merge_objects(
        &self,
        left: &serde_json::Map<String, serde_json::Value>,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        let mut merged = left.clone();

        if let Value::Object(right_obj) = right {
            for (k, v) in right_obj {
                merged.insert(k.clone(), v.clone());
            }
        }

        Ok(Value::Object(merged))
    }

    /// Apply a map transformation to array data
    ///
    /// WHY: Often need to extract specific fields from arrays or transform data.
    /// Example: Transform array of orders to just extract totals
    ///
    /// SUPPORTED TRANSFORMS:
    /// - "field": Extract a field from each object: ${orders | map(field=total)}
    /// - "first": Get first element
    /// - "last": Get last element
    /// - "length": Get array length
    fn apply_map_transform(
        &self,
        source: &str,
        transform: &str,
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        let data = self.resolve_path(source, results)?;

        // Parse transform string to extract operation
        // Examples: "field=total", "first", "last", "length"

        if transform == "first" {
            // Get first element
            if let Value::Array(arr) = data {
                return Ok(arr.first().cloned().unwrap_or(Value::Null));
            }
            return Ok(data);
        }

        if transform == "last" {
            // Get last element
            if let Value::Array(arr) = data {
                return Ok(arr.last().cloned().unwrap_or(Value::Null));
            }
            return Ok(data);
        }

        if transform == "length" || transform == "count" {
            // Get array length
            if let Value::Array(arr) = data {
                return Ok(Value::Number(serde_json::Number::from(arr.len())));
            }
            return Ok(Value::Number(serde_json::Number::from(0)));
        }

        // Handle field extraction: "field=fieldName"
        if let Some(field_name) = transform.strip_prefix("field=") {
            if let Value::Array(arr) = data {
                let mapped: Vec<Value> = arr
                    .iter()
                    .filter_map(|item| item.get(field_name).cloned())
                    .collect();
                return Ok(Value::Array(mapped));
            }
        }

        // If no transform matches, return original data
        Ok(data)
    }

    /// Apply a filter transformation to array data
    ///
    /// WHY: Need to filter arrays based on conditions
    /// Example: Get only active orders
    ///
    /// SUPPORTED CONDITIONS:
    /// - "field=value": Filter where field equals value
    /// - "field!=value": Filter where field not equals value
    /// - "field>value": Filter where field greater than value
    /// - "field<value": Filter where field less than value
    fn apply_filter_transform(
        &self,
        source: &str,
        condition: &str,
        results: &HashMap<String, DataSourceResult>,
    ) -> Result<serde_json::Value> {
        use serde_json::Value;

        let data = self.resolve_path(source, results)?;

        if let Value::Array(arr) = data {
            // Parse condition: "status=active", "total>100", etc.
            let filtered: Vec<Value> = if let Some((field, value)) = condition.split_once('=') {
                let field = field.trim();
                let value = value.trim();

                if field.ends_with('!') {
                    // Not equals: "status!=pending"
                    let field = field.trim_end_matches('!');
                    arr.iter()
                        .filter(|item| {
                            if let Some(item_value) = item.get(field) {
                                !Self::compare_values(item_value, value, "eq")
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect()
                } else if field.ends_with('>') {
                    // Greater than: "total>=100"
                    let field = field.trim_end_matches('>');
                    arr.iter()
                        .filter(|item| {
                            if let Some(item_value) = item.get(field) {
                                Self::compare_values(item_value, value, "gt")
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect()
                } else if field.ends_with('<') {
                    // Less than: "total<=50"
                    let field = field.trim_end_matches('<');
                    arr.iter()
                        .filter(|item| {
                            if let Some(item_value) = item.get(field) {
                                Self::compare_values(item_value, value, "lt")
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect()
                } else {
                    // Equals: "status=active"
                    arr.iter()
                        .filter(|item| {
                            if let Some(item_value) = item.get(field) {
                                Self::compare_values(item_value, value, "eq")
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect()
                }
            } else {
                arr.clone()
            };

            Ok(Value::Array(filtered))
        } else {
            Ok(data)
        }
    }

    /// Compare JSON values
    fn compare_values(item_value: &serde_json::Value, compare_to: &str, op: &str) -> bool {
        use serde_json::Value;

        match item_value {
            Value::String(s) => match op {
                "eq" => s == compare_to,
                _ => false,
            },
            Value::Number(n) => {
                if let Ok(compare_num) = compare_to.parse::<f64>() {
                    if let Some(item_num) = n.as_f64() {
                        match op {
                            "eq" => (item_num - compare_num).abs() < f64::EPSILON,
                            "gt" => item_num > compare_num,
                            "lt" => item_num < compare_num,
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Value::Bool(b) => match op {
                "eq" => {
                    let compare_bool = compare_to.parse::<bool>().unwrap_or(false);
                    b == &compare_bool
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn apply_filter(
        &self,
        _data: &serde_json::Value,
        _filter: &Filter,
    ) -> Result<serde_json::Value> {
        // TODO: Implement filtering for post-processing
        Ok(_data.clone())
    }
}

// TODO: Future enhancements
// ========================
// 1. TCP/UDP Executor: For custom protocols (Memcached, legacy systems, proprietary protocols)
//    - Useful but adds security complexity
//    - Most modern systems expose HTTP APIs
//    - Consider wrapping custom protocols with HTTP proxy instead
//
// 2. gRPC Executor: For gRPC services
//    - Would need protobuf schema management
//    - Consider for Phase 3 if there's demand
//
// 3. MQTT Executor: For IoT pub/sub
//    - Useful for IoT dashboards
//    - Consider for Phase 3
//
// 4. WebSocket Executor: For real-time data
//    - Different from subscriptions (which are server-push)
//    - Consider for Phase 3

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_simple() {
        let composer = ResponseComposer::new();

        let mut results = HashMap::new();
        results.insert(
            "user".to_string(),
            DataSourceResult {
                source_id: "user".to_string(),
                data: serde_json::json!({"id": 1, "name": "John"}),
                metadata: ExecutionMetadata {
                    duration_ms: 10,
                    from_cache: false,
                    retries: 0,
                    was_batched: false,
                    warnings: vec![],
                },
            },
        );

        let template = serde_json::json!({
            "user": "${user}",
            "name": "${user.name}"
        });

        let result = composer
            .compose(results, &CompositionTemplate::Template(template))
            .unwrap();

        assert_eq!(result["user"]["id"], 1);
        assert_eq!(result["name"], "John");
    }

    #[test]
    fn test_cross_source_join() {
        let composer = ResponseComposer::new();

        let mut results = HashMap::new();

        // Orders from MongoDB
        results.insert(
            "orders".to_string(),
            DataSourceResult {
                source_id: "orders".to_string(),
                data: serde_json::json!([
                    {"id": "1", "user_id": "1", "total": 100},
                    {"id": "2", "user_id": "1", "total": 200}
                ]),
                metadata: ExecutionMetadata {
                    duration_ms: 10,
                    from_cache: false,
                    retries: 0,
                    was_batched: false,
                    warnings: vec![],
                },
            },
        );

        // Shipping status from REST API
        results.insert(
            "shipping".to_string(),
            DataSourceResult {
                source_id: "shipping".to_string(),
                data: serde_json::json!([
                    {"id": "1", "status": "shipped"},
                    {"id": "2", "status": "pending"}
                ]),
                metadata: ExecutionMetadata {
                    duration_ms: 50,
                    from_cache: false,
                    retries: 0,
                    was_batched: false,
                    warnings: vec![],
                },
            },
        );

        let mut fields = HashMap::new();
        fields.insert(
            "orders_with_shipping".to_string(),
            FieldTransform::Join {
                left: "orders".to_string(),
                right: "shipping".to_string(),
                left_key: "id".to_string(),
                right_key: "id".to_string(),
                join_type: JoinType::Inner,
            },
        );

        let template = CompositionTemplate::Advanced {
            fields,
            filters: None,
        };

        let result = composer.compose(results, &template).unwrap();

        // Verify join worked
        assert!(result["orders_with_shipping"].is_array());
        let orders = result["orders_with_shipping"].as_array().unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0]["status"], "shipped");
        assert_eq!(orders[1]["status"], "pending");
    }
}
