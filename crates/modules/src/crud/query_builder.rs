use ude_core::*;
use serde_json::Value;

pub struct SqlQueryBuilder<'a> {
    db_type: &'a DbType,
}

impl<'a> SqlQueryBuilder<'a> {
    pub fn new(db_type: &'a DbType) -> Self {
        Self { db_type }
    }

    /// Build SELECT query from ReadRequest
    pub fn build_select(&self, table: &str, req: &ReadRequest) -> Result<(String, Vec<Value>)> {
        let mut sql = String::from("SELECT ");
        let mut params = Vec::new();

        // SELECT clause
        match &req.options.select {
            Some(Value::Object(fields)) => {
                let selected: Vec<String> = fields
                    .iter()
                    .filter(|(_, v)| v.as_i64().unwrap_or(0) == 1)
                    .map(|(k, _)| self.quote_identifier(k))
                    .collect();

                if selected.is_empty() {
                    sql.push('*');
                } else {
                    sql.push_str(&selected.join(", "));
                }
            }
            _ => sql.push('*'),
        }

        sql.push_str(&format!(" FROM {}", self.quote_identifier(table)));

        // WHERE clause
        let (where_clause, where_params) = self.build_where(&req.find)?;
        if !where_clause.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clause);
            params.extend(where_params);
        }

        // ORDER BY clause
        if !req.options.sort.is_empty() {
            sql.push_str(" ORDER BY ");
            let sort_parts: Vec<String> = req
                .options
                .sort
                .iter()
                .map(|s| {
                    if let Some(field) = s.strip_prefix('-') {
                        format!("{} DESC", self.quote_identifier(field))
                    } else {
                        format!("{} ASC", self.quote_identifier(s))
                    }
                })
                .collect();
            sql.push_str(&sort_parts.join(", "));
        }

        // LIMIT
        if let Some(limit) = req.options.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        // OFFSET
        if req.options.skip > 0 {
            sql.push_str(&format!(" OFFSET {}", req.options.skip));
        }

        Ok((sql, params))
    }

    /// Build INSERT query from CreateRequest
    pub fn build_insert(&self, table: &str, req: &CreateRequest) -> Result<(String, Vec<Value>)> {
        let doc = match &req.doc {
            Value::Object(map) => map,
            Value::Array(_) => {
                return Err(Error::Validation {
                    field: "doc".to_string(),
                    message: "Batch insert not yet supported, use single document".to_string(),
                });
            }
            _ => {
                return Err(Error::Validation {
                    field: "doc".to_string(),
                    message: "Document must be an object".to_string(),
                });
            }
        };

        let mut columns = Vec::new();
        let mut placeholders = Vec::new();
        let mut params = Vec::new();

        for (idx, (key, value)) in doc.iter().enumerate() {
            columns.push(self.quote_identifier(key));
            placeholders.push(self.placeholder(idx + 1));
            params.push(value.clone());
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.quote_identifier(table),
            columns.join(", "),
            placeholders.join(", ")
        );

        Ok((sql, params))
    }

    /// Build UPDATE query from UpdateRequest
    pub fn build_update(&self, table: &str, req: &UpdateRequest) -> Result<(String, Vec<Value>)> {
        let mut sql = format!("UPDATE {}", self.quote_identifier(table));
        let mut params = Vec::new();

        // SET clause
        let update_doc = match &req.update {
            Value::Object(map) => {
                if let Some(Value::Object(set_fields)) = map.get("$set") {
                    set_fields
                } else {
                    // Direct update: {name: "John"}
                    map
                }
            }
            _ => {
                return Err(Error::Validation {
                    field: "update".to_string(),
                    message: "Update document must be an object".to_string(),
                });
            }
        };

        let set_parts: Vec<String> = update_doc
            .iter()
            .enumerate()
            .map(|(idx, (key, value))| {
                params.push(value.clone());
                format!("{} = {}", self.quote_identifier(key), self.placeholder(idx + 1))
            })
            .collect();

        if set_parts.is_empty() {
            return Err(Error::Validation {
                field: "update".to_string(),
                message: "No fields to update".to_string(),
            });
        }

        sql.push_str(" SET ");
        sql.push_str(&set_parts.join(", "));

        // WHERE clause
        let param_offset = params.len() + 1;
        let (where_clause, where_params) = self.build_where_with_offset(&req.find, param_offset)?;

        if !where_clause.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clause);
            params.extend(where_params);
        }

        Ok((sql, params))
    }

    /// Build DELETE query from DeleteRequest
    pub fn build_delete(&self, table: &str, req: &DeleteRequest) -> Result<(String, Vec<Value>)> {
        let mut sql = format!("DELETE FROM {}", self.quote_identifier(table));
        let mut params = Vec::new();

        // WHERE clause
        let (where_clause, where_params) = self.build_where(&req.find)?;

        if !where_clause.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clause);
            params.extend(where_params);
        } else {
            // Require WHERE clause for DELETE to prevent accidental full table deletes
            return Err(Error::Validation {
                field: "find".to_string(),
                message: "DELETE requires a WHERE clause. Use {\"1\": 1} to delete all rows".to_string(),
            });
        }

        Ok((sql, params))
    }

    /// Build WHERE clause from find JSON
    fn build_where(&self, find: &Value) -> Result<(String, Vec<Value>)> {
        self.build_where_with_offset(find, 1)
    }

    fn build_where_with_offset(&self, find: &Value, param_offset: usize) -> Result<(String, Vec<Value>)> {
        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = param_offset;

        match find {
            Value::Object(map) => {
                for (key, value) in map {
                    match value {
                        Value::Object(op_map) => {
                            // Operator query: {age: {">": 18}}
                            for (op, val) in op_map {
                                let sql_op = self.map_operator(op)?;

                                if op == "in" || op == "notIn" {
                                    // IN operator: {age: {"in": [18, 21, 25]}}
                                    let values = val.as_array().ok_or_else(|| Error::Validation {
                                        field: key.clone(),
                                        message: "IN operator requires an array".to_string(),
                                    })?;

                                    let placeholders: Vec<String> = (0..values.len())
                                        .map(|i| {
                                            params.push(values[i].clone());
                                            let ph = self.placeholder(param_idx);
                                            param_idx += 1;
                                            ph
                                        })
                                        .collect();

                                    clauses.push(format!(
                                        "{} {} ({})",
                                        self.quote_identifier(key),
                                        sql_op,
                                        placeholders.join(", ")
                                    ));
                                } else {
                                    clauses.push(format!(
                                        "{} {} {}",
                                        self.quote_identifier(key),
                                        sql_op,
                                        self.placeholder(param_idx)
                                    ));
                                    params.push(val.clone());
                                    param_idx += 1;
                                }
                            }
                        }
                        _ => {
                            // Equality: {name: "John"}
                            clauses.push(format!(
                                "{} = {}",
                                self.quote_identifier(key),
                                self.placeholder(param_idx)
                            ));
                            params.push(value.clone());
                            param_idx += 1;
                        }
                    }
                }
            }
            _ => {
                return Err(Error::Validation {
                    field: "find".to_string(),
                    message: "Find clause must be an object".to_string(),
                });
            }
        }

        Ok((clauses.join(" AND "), params))
    }

    fn map_operator(&self, op: &str) -> Result<&'static str> {
        match op {
            ">" => Ok(">"),
            ">=" => Ok(">="),
            "<" => Ok("<"),
            "<=" => Ok("<="),
            "!=" => Ok("!="),
            "in" => Ok("IN"),
            "notIn" => Ok("NOT IN"),
            _ => Err(Error::Validation {
                field: "operator".to_string(),
                message: format!("Unknown operator: {}", op),
            }),
        }
    }

    fn placeholder(&self, idx: usize) -> String {
        match self.db_type {
            DbType::Postgres => format!("${}", idx),
            DbType::Mysql => "?".to_string(),
            DbType::Sqlserver => format!("@p{}", idx),
            _ => "?".to_string(),
        }
    }

    fn quote_identifier(&self, id: &str) -> String {
        match self.db_type {
            DbType::Postgres => format!("\"{}\"", id),
            DbType::Mysql => format!("`{}`", id),
            DbType::Sqlserver => format!("[{}]", id),
            _ => format!("\"{}\"", id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_select_simple() {
        let builder = SqlQueryBuilder::new(&DbType::Postgres);
        let req = ReadRequest {
            find: json!({"name": "John"}),
            options: ReadOptions::default(),
        };

        let (sql, params) = builder.build_select("users", &req).unwrap();
        assert_eq!(sql, r#"SELECT * FROM "users" WHERE "name" = $1"#);
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_build_select_with_operators() {
        let builder = SqlQueryBuilder::new(&DbType::Postgres);
        let req = ReadRequest {
            find: json!({"age": {">": 18}}),
            options: ReadOptions::default(),
        };

        let (sql, params) = builder.build_select("users", &req).unwrap();
        assert_eq!(sql, r#"SELECT * FROM "users" WHERE "age" > $1"#);
        assert_eq!(params[0], json!(18));
    }

    #[test]
    fn test_build_insert() {
        let builder = SqlQueryBuilder::new(&DbType::Postgres);
        let req = CreateRequest {
            op: CreateOp::One,
            doc: json!({"name": "John", "age": 30}),
        };

        let (sql, params) = builder.build_insert("users", &req).unwrap();
        assert!(sql.starts_with(r#"INSERT INTO "users""#));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_update() {
        let builder = SqlQueryBuilder::new(&DbType::Postgres);
        let req = UpdateRequest {
            find: json!({"id": 1}),
            update: json!({"$set": {"name": "Jane"}}),
            op: UpdateOp::Set,
        };

        let (sql, params) = builder.build_update("users", &req).unwrap();
        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("SET"));
        assert!(sql.contains("WHERE"));
        assert_eq!(params.len(), 2);
    }
}
