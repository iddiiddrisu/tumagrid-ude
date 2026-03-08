# OpenTelemetry Integration Guide

## 🎯 Overview

This document explains how to integrate UDE with your managed monitoring service using OpenTelemetry (OTel).

**What You Get:**
- ✅ **Distributed Tracing** - Request flow across services
- ✅ **Metrics** - Request rates, latencies, error rates, business metrics
- ✅ **Structured Logging** - JSON logs with correlation IDs
- ✅ **Auto-instrumentation** - HTTP requests automatically traced
- ✅ **Custom Metrics** - Orchestration performance, database stats

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│  UDE Application                                 │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐│
│  │   Traces     │  │   Metrics    │  │     Logs      ││
│  │  (Requests)  │  │ (Counters/   │  │  (Structured) ││
│  │              │  │  Histograms) │  │     JSON      ││
│  └──────┬───────┘  └──────┬───────┘  └───────┬───────┘│
│         │                  │                   │        │
│         └──────────────────┴───────────────────┘        │
│                            │                            │
│                   ┌────────▼────────┐                  │
│                   │  OTel SDK       │                  │
│                   │  (Rust)         │                  │
│                   └────────┬────────┘                  │
└────────────────────────────┼────────────────────────────┘
                             │
                    ┌────────▼────────┐
                    │ OTel Collector  │  ← Your Managed Service
                    │   (Optional)    │
                    └────────┬────────┘
                             │
            ┌────────────────┼────────────────┐
            │                │                │
    ┌───────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐
    │  Prometheus  │ │   Jaeger    │ │    Loki     │
    │  (Metrics)   │ │  (Traces)   │ │   (Logs)    │
    └──────────────┘ └─────────────┘ └─────────────┘
            │                │                │
            └────────────────┼────────────────┘
                             │
                    ┌────────▼────────┐
                    │  Grafana        │  ← Visualization
                    │  Dashboard      │
                    └─────────────────┘
```

---

## 🚀 Quick Start

### Option 1: Prometheus + Jaeger (Self-hosted)

**1. Start monitoring stack:**
```bash
# docker-compose.yml
version: '3.8'
services:
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # Jaeger UI
      - "4317:4317"    # OTLP gRPC
      - "4318:4318"    # OTLP HTTP

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
```

**2. Configure UDE:**
```bash
# Environment variables
export OTEL_SERVICE_NAME=spaceforge
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_TRACES_EXPORTER=otlp
export OTEL_METRICS_EXPORTER=prometheus
export OTEL_LOGS_EXPORTER=otlp

# Run UDE
./gateway --config config.yaml
```

**3. Access dashboards:**
- Metrics: http://localhost:9090 (Prometheus)
- Traces: http://localhost:16686 (Jaeger)
- Dashboards: http://localhost:3000 (Grafana)

### Option 2: OpenTelemetry Collector (Recommended)

**1. Deploy OTel Collector:**
```yaml
# otel-collector-config.yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:
    timeout: 10s

  memory_limiter:
    check_interval: 1s
    limit_mib: 512

exporters:
  prometheus:
    endpoint: "0.0.0.0:8889"

  jaeger:
    endpoint: jaeger:14250
    tls:
      insecure: true

  loki:
    endpoint: http://loki:3100/loki/api/v1/push

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [jaeger]

    metrics:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [prometheus]

    logs:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [loki]
```

**2. Run collector:**
```bash
docker run -p 4317:4317 -p 4318:4318 \
  -v $(pwd)/otel-collector-config.yaml:/etc/otel/config.yaml \
  otel/opentelemetry-collector-contrib:latest \
  --config /etc/otel/config.yaml
```

### Option 3: Grafana Cloud (Managed)

**1. Get credentials from Grafana Cloud:**
- Prometheus endpoint
- Loki endpoint
- Tempo endpoint
- API key

**2. Configure collector:**
```yaml
# otel-collector-grafana-cloud.yaml
exporters:
  prometheusremotewrite:
    endpoint: https://prometheus-xxx.grafana.net/api/prom/push
    headers:
      authorization: "Bearer ${GRAFANA_API_KEY}"

  otlp/tempo:
    endpoint: tempo-xxx.grafana.net:443
    headers:
      authorization: "Bearer ${GRAFANA_API_KEY}"

  loki:
    endpoint: https://logs-xxx.grafana.net/loki/api/v1/push
    headers:
      authorization: "Bearer ${GRAFANA_API_KEY}"
```

---

## 📊 Available Metrics

### HTTP Metrics

```
# Request rate
http_requests_total{method="POST", path="/orchestration", status="200"}

# Request latency (histogram)
http_request_duration_seconds{method="POST", path="/orchestration", status="200"}
# Quantiles: p50, p95, p99

# Error rate
http_errors_total{method="POST", path="/orchestration", status="500"}
```

### CRUD Metrics

```
# Operation count
crud_operations_total{operation="read", db="postgres", collection="users"}

# Operation latency
crud_operation_duration_seconds{operation="read", db="postgres"}

# Error count
crud_errors_total{operation="read", db="postgres", collection="users"}
```

### Orchestration Metrics (The Killer Feature!)

```
# Query execution count
orchestration_queries_total{query_id="ecommerce_homepage"}

# Query duration
orchestration_query_duration_seconds{query_id="ecommerce_homepage"}
# p50, p95, p99 quantiles

# Data sources executed per query
orchestration_sources_executed{query_id="ecommerce_homepage"}

# Execution stages per query
orchestration_stages_executed{query_id="ecommerce_homepage"}

# Cache hits
orchestration_cache_hits_total{query_id="ecommerce_homepage"}

# Error count
orchestration_errors_total{query_id="ecommerce_homepage", error_type="timeout"}
```

### Database Metrics

```
# Active connections
db_connections_active{db="postgres", pool="main"}

# Query duration
db_query_duration_seconds{db="postgres", operation="SELECT"}

# Error count
db_errors_total{db="postgres", error_type="connection_failed"}
```

---

## 🔍 Distributed Tracing

### Trace Structure

```
Span: HTTP Request
├─ Span: orchestration.execute_query
│  ├─ Span: orchestration.plan
│  ├─ Span: orchestration.stage_1
│  │  ├─ Span: database.read (postgres.users)
│  │  └─ Span: cache.get (user:123:prefs)
│  ├─ Span: orchestration.stage_2
│  │  ├─ Span: database.read (mongodb.orders)
│  │  ├─ Span: rest_api.call (shipping-api)
│  │  └─ Span: graphql.query (product-catalog)
│  └─ Span: orchestration.compose
└─ End
```

### Custom Span Attributes

**Orchestration spans:**
```
orchestration.query_id = "ecommerce_homepage"
orchestration.num_sources = 5
orchestration.num_stages = 3
orchestration.cache_used = true
orchestration.total_duration_ms = 450
```

**Database spans:**
```
db.system = "postgresql"
db.name = "myapp_db"
db.operation = "SELECT"
db.statement = "SELECT * FROM users WHERE id = $1"
db.rows_affected = 1
```

**HTTP spans:**
```
http.method = "POST"
http.url = "/v1/api/myapp/orchestration/ecommerce_homepage"
http.status_code = 200
http.request.body.size = 1024
http.response.body.size = 4096
```

---

## 📝 Structured Logging

### Log Format

```json
{
  "timestamp": "2025-10-30T14:00:00.123Z",
  "level": "INFO",
  "target": "spaceforge::orchestration",
  "message": "Executing composite query",
  "fields": {
    "query_id": "ecommerce_homepage",
    "num_sources": 5,
    "trace_id": "a1b2c3d4e5f6g7h8",
    "span_id": "1a2b3c4d"
  }
}
```

### Log Levels

- **ERROR**: Unrecoverable errors (failed requests, crashes)
- **WARN**: Recoverable issues (slow queries, retries, degraded performance)
- **INFO**: Important events (query execution, config changes)
- **DEBUG**: Detailed debugging (query plans, execution steps)
- **TRACE**: Verbose debugging (all internal operations)

### Correlation

All logs include:
- `trace_id`: Links logs to distributed traces
- `span_id`: Links logs to specific span
- `request_id`: Unique per HTTP request
- `project_id`: Multi-tenant project identifier

---

## 📈 Grafana Dashboards

### Dashboard 1: Overview

**Panels:**
- Total requests/sec (time series)
- Error rate % (gauge)
- P95 latency (time series)
- Active database connections (gauge)

**Queries:**
```promql
# Request rate
rate(http_requests_total[5m])

# Error rate
rate(http_errors_total[5m]) / rate(http_requests_total[5m])

# P95 latency
histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))

# Active connections
db_connections_active
```

### Dashboard 2: Orchestration Performance

**Panels:**
- Query execution rate (time series)
- Query duration P50/P95/P99 (graph)
- Sources executed per query (histogram)
- Cache hit rate % (gauge)

**Queries:**
```promql
# Query rate by query_id
rate(orchestration_queries_total[5m])

# Query duration quantiles
histogram_quantile(0.95, rate(orchestration_query_duration_seconds_bucket[5m]))

# Cache hit rate
rate(orchestration_cache_hits_total[5m]) /
rate(orchestration_queries_total[5m])

# Sources executed
orchestration_sources_executed
```

### Dashboard 3: Database Health

**Panels:**
- Query rate by database (stacked area)
- Query duration by operation (heatmap)
- Connection pool usage (gauge)
- Error rate by database (time series)

---

## ⚙️ Configuration

### Environment Variables

```bash
# Service identification
export OTEL_SERVICE_NAME=spaceforge
export OTEL_SERVICE_VERSION=0.1.0
export DEPLOYMENT_ENVIRONMENT=production

# OTLP exporter
export OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
export OTEL_EXPORTER_OTLP_TIMEOUT=10000  # ms

# Sampling
export OTEL_TRACES_SAMPLER=parentbased_traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1  # Sample 10% in production

# Prometheus
export OTEL_METRICS_EXPORTER=prometheus
export PROMETHEUS_PORT=9090

# Logging
export RUST_LOG=info
export LOG_FORMAT=json
```

### Configuration File

```yaml
# config.yaml
telemetry:
  service_name: spaceforge
  service_version: 0.1.0
  environment: production

  # OTLP endpoint for traces and metrics
  otlp_endpoint: http://otel-collector:4317

  # Enable Prometheus metrics endpoint
  enable_prometheus: true
  prometheus_port: 9090

  # Trace sampling (0.0 to 1.0)
  trace_sample_rate: 0.1  # 10% in production

  # Log level
  log_level: info
```

---

## 🚨 Alerts

### Critical Alerts

**1. High Error Rate**
```promql
# Alert if error rate > 5% for 5 minutes
(
  rate(http_errors_total[5m]) /
  rate(http_requests_total[5m])
) > 0.05
```

**2. High Latency**
```promql
# Alert if P95 latency > 1s for 10 minutes
histogram_quantile(0.95,
  rate(http_request_duration_seconds_bucket[10m])
) > 1.0
```

**3. Database Connection Pool Exhausted**
```promql
# Alert if >90% connections used
db_connections_active / db_connections_max > 0.9
```

**4. Orchestration Query Failures**
```promql
# Alert if >10 failures in 5 minutes
rate(orchestration_errors_total[5m]) > 10
```

### Warning Alerts

**1. Slow Queries**
```promql
# Warn if P95 > 500ms
histogram_quantile(0.95,
  rate(orchestration_query_duration_seconds_bucket[5m])
) > 0.5
```

**2. Low Cache Hit Rate**
```promql
# Warn if cache hit rate < 50%
(
  rate(orchestration_cache_hits_total[5m]) /
  rate(orchestration_queries_total[5m])
) < 0.5
```

---

## 🔧 Integration with Popular Backends

### Datadog

```bash
# Install Datadog agent with OTLP support
docker run -d \
  --name datadog-agent \
  -e DD_API_KEY=${DD_API_KEY} \
  -e DD_OTLP_CONFIG_RECEIVER_PROTOCOLS_GRPC_ENDPOINT=0.0.0.0:4317 \
  -p 4317:4317 \
  datadog/agent:latest

# Configure UDE
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

### New Relic

```bash
# Get OTLP endpoint from New Relic UI
export OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp.nr-data.net:4317
export OTEL_EXPORTER_OTLP_HEADERS="api-key=${NEW_RELIC_LICENSE_KEY}"
```

### Honeycomb

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io
export OTEL_EXPORTER_OTLP_HEADERS="x-honeycomb-team=${HONEYCOMB_API_KEY}"
```

---

## 📊 Example Queries

### Top 10 Slowest Queries

```promql
topk(10,
  histogram_quantile(0.95,
    rate(orchestration_query_duration_seconds_bucket[5m])
  ) by (query_id)
)
```

### Error Rate by Endpoint

```promql
rate(http_errors_total[5m]) by (path)
```

### Database Query Latency

```promql
histogram_quantile(0.99,
  rate(db_query_duration_seconds_bucket[5m])
) by (db, operation)
```

### Cache Hit Rate Over Time

```promql
rate(orchestration_cache_hits_total[5m]) /
rate(orchestration_queries_total[5m])
```

---

## 🎯 Best Practices

### 1. Sampling in Production
- **Development**: Sample 100% (trace_sample_rate = 1.0)
- **Staging**: Sample 50% (trace_sample_rate = 0.5)
- **Production**: Sample 10-20% (trace_sample_rate = 0.1-0.2)

### 2. Log Levels
- **Production**: INFO or WARN
- **Staging**: DEBUG
- **Troubleshooting**: TRACE (temporarily)

### 3. Metrics Cardinality
- Avoid high-cardinality labels (user IDs, email addresses)
- Use query_id, db_name, operation_type instead
- Monitor metrics cardinality in Prometheus

### 4. Retention
- **Metrics**: 30 days minimum
- **Traces**: 7-14 days
- **Logs**: 30 days (ERROR/WARN), 7 days (INFO/DEBUG)

### 5. Alerting
- Alert on symptoms, not causes
- Define clear SLOs (Service Level Objectives)
- Example SLOs:
  - 99.9% availability
  - P95 latency < 500ms
  - Error rate < 1%

---

## 🚀 Next Steps for IaaS Team

### Week 1: Setup
- [ ] Deploy OTel Collector or choose managed service
- [ ] Configure UDE with OTLP endpoint
- [ ] Verify traces/metrics/logs flowing

### Week 2: Dashboards
- [ ] Create Grafana dashboards
- [ ] Import provided dashboard JSON
- [ ] Customize for your needs

### Week 3: Alerts
- [ ] Set up critical alerts
- [ ] Configure alert channels (Slack, PagerDuty)
- [ ] Test alert firing

### Month 1: Production
- [ ] Tune sampling rates
- [ ] Optimize retention policies
- [ ] Document runbooks

---

## 📞 Support

**Documentation:**
- OpenTelemetry: https://opentelemetry.io/docs
- Prometheus: https://prometheus.io/docs
- Grafana: https://grafana.com/docs

**Questions?**
Contact the backend team for UDE-specific telemetry questions.

---

**Document Version**: 1.0
**Last Updated**: 2025-10-30
**Status**: Ready for Integration
