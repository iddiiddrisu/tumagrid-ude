# Service Mesh Quick Start Guide

## What is the Service Mesh?

The Service Mesh is SpaceCloud's intelligent routing layer for **services you deployed and control**. It provides:

- **Smart Routing**: Automatic lowest-latency routing based on real-time measurements
- **Health Monitoring**: Continuous health checks with automatic failover
- **Multi-Region**: Deploy once, route intelligently across all regions
- **Zero Config**: Tenants get mesh benefits automatically, no code changes

## Critical Distinction

**Use ServiceMesh for:**
- Services YOU deployed via runner
- Partner services hosted on YOUR infrastructure
- Services in YOUR orchestration cluster

**Use RestApi for:**
- External APIs (Stripe, Twilio, etc.)
- Legacy VMs not in your mesh
- Services not deployed by your runner

---

## Quick Start: Register Your First Service

### Step 1: Start the Gateway

```bash
cd /home/iddi/Documents/code/Iddi/code/tumagrid
cargo run --bin gateway
```

The mesh starts automatically with:
- Health checker (probes every 10s)
- Latency tracker (measures every 10s)
- Smart router (ready to route)

### Step 2: Register a Service

```bash
curl -X POST http://localhost:8080/mesh/services \
  -H "Content-Type: application/json" \
  -d '{
    "id": "hubtel-payments",
    "name": "Hubtel Payments API",
    "category": "payments",
    "endpoints": [
      {
        "id": "hubtel-accra-1",
        "service_id": "hubtel-payments",
        "url": "https://accra-1.hubtel.internal",
        "region": "accra",
        "health": "unknown",
        "latency": {
          "p50_ms": 9999,
          "p95_ms": 9999,
          "p99_ms": 9999,
          "last_ms": 9999,
          "updated_at": "2025-01-01T00:00:00Z"
        },
        "deployed_at": "2025-01-01T00:00:00Z",
        "weight": 1.0
      },
      {
        "id": "hubtel-lagos-1",
        "service_id": "hubtel-payments",
        "url": "https://lagos-1.hubtel.internal",
        "region": "lagos",
        "health": "unknown",
        "latency": {
          "p50_ms": 9999,
          "p95_ms": 9999,
          "p99_ms": 9999,
          "last_ms": 9999,
          "updated_at": "2025-01-01T00:00:00Z"
        },
        "deployed_at": "2025-01-01T00:00:00Z",
        "weight": 1.0
      }
    ],
    "config": {
      "default_timeout_ms": 30000,
      "health_check": {
        "path": "/health",
        "interval_secs": 10,
        "timeout_ms": 5000,
        "expected_status": [200]
      },
      "requires_auth": true,
      "rate_limit": null
    }
  }'
```

**Response:**
```json
{
  "service_id": "hubtel-payments",
  "message": "Service registered successfully"
}
```

### Step 3: List Registered Services

```bash
curl http://localhost:8080/mesh/services
```

**Response:**
```json
{
  "count": 1,
  "services": [
    {
      "id": "hubtel-payments",
      "name": "Hubtel Payments API",
      "category": "payments",
      "endpoint_count": 2,
      "registered_at": "2025-01-24T09:26:00Z"
    }
  ]
}
```

### Step 4: Check Service Health

```bash
curl http://localhost:8080/mesh/services/hubtel-payments/health
```

**Response:**
```json
{
  "service_id": "hubtel-payments",
  "stats": {
    "total": 2,
    "healthy": 2,
    "degraded": 0,
    "unhealthy": 0,
    "unknown": 0,
    "availability_percent": 100.0
  },
  "endpoints": [
    {
      "id": "hubtel-accra-1",
      "url": "https://accra-1.hubtel.internal",
      "region": "accra",
      "health": "healthy"
    },
    {
      "id": "hubtel-lagos-1",
      "url": "https://lagos-1.hubtel.internal",
      "region": "lagos",
      "health": "healthy"
    }
  ]
}
```

### Step 5: Check Latency Stats

```bash
curl http://localhost:8080/mesh/services/hubtel-payments/latency
```

**Response:**
```json
{
  "service_id": "hubtel-payments",
  "endpoints": [
    {
      "endpoint_id": "hubtel-accra-1",
      "url": "https://accra-1.hubtel.internal",
      "region": "accra",
      "stats": {
        "p50_ms": 12,
        "p95_ms": 18,
        "p99_ms": 25,
        "last_ms": 13,
        "updated_at": "2025-01-24T09:30:00Z"
      }
    },
    {
      "endpoint_id": "hubtel-lagos-1",
      "url": "https://lagos-1.hubtel.internal",
      "region": "lagos",
      "stats": {
        "p50_ms": 45,
        "p95_ms": 62,
        "p99_ms": 89,
        "last_ms": 47,
        "updated_at": "2025-01-24T09:30:00Z"
      }
    }
  ]
}
```

---

## Using the Mesh in Orchestration Queries

### Tenant Config (YAML)

```yaml
# Composite query using mesh service
composite_queries:
  checkout:
    sources:
      # Database query (as before)
      - id: order
        type: database
        db_alias: main
        collection: orders
        find:
          id: "${args.order_id}"

      # Service mesh call (NEW!)
      - id: payment
        type: servicemesh          # ← Uses smart routing
        service: hubtel-payments   # ← Service ID in registry
        path: /v1/charge
        method: POST
        headers:
          Authorization: "Bearer ${env.HUBTEL_TOKEN}"
        body:
          amount: "${order.amount}"
          phone: "${order.customer_phone}"
          reference: "${order.id}"
        # Optional: routing preference
        routing_preference:
          type: lowest_latency     # Default, picks fastest endpoint

      # External API (still works!)
      - id: stripe
        type: restapi              # ← No mesh, just retry
        url: https://api.stripe.com/v1/charges
        method: POST
        # ... rest of config

    response:
      order: "${order}"
      payment: "${payment}"
      timestamp: "${now()}"
```

### What Happens Behind the Scenes

1. **Tenant calls:** `POST /v1/api/my-project/orchestration/checkout`
2. **Query executor sees:** `type: servicemesh`
3. **ServiceMeshExecutor:**
   - Asks router: "Which endpoint for hubtel-payments?"
   - Router checks:
     - Accra: healthy, 12ms p50 ← **PICKS THIS**
     - Lagos: healthy, 45ms p50
   - Routes to: `https://accra-1.hubtel.internal/v1/charge`
4. **Request executes** with retry logic
5. **Response flows back** to tenant

**If Accra endpoint goes down:**
- Health checker marks it unhealthy
- Router automatically uses Lagos instead
- Tenant request succeeds (failover is automatic)

---

## Routing Strategies

### Lowest Latency (Default)

```yaml
routing_preference:
  type: lowest_latency
```

Picks the endpoint with lowest p50 latency. Best for most use cases.

### Region Affinity

```yaml
routing_preference:
  type: region_affinity
  preferred_region: eu-west
```

Prefers endpoints in `eu-west` region. Falls back to lowest latency if none available.

**Use case:** GDPR compliance, data locality.

### Round Robin

```yaml
routing_preference:
  type: round_robin
```

Evenly distributes requests across all healthy endpoints.

**Use case:** Background jobs, bulk operations.

### Weighted

```yaml
# Set weights when registering endpoints
endpoints:
  - id: new-version
    weight: 0.1   # 10% of traffic (canary)
  - id: stable
    weight: 0.9   # 90% of traffic
```

Then use:
```yaml
routing_preference:
  type: weighted
```

**Use case:** Canary deployments, gradual rollouts.

---

## API Reference

### List Services
```
GET /mesh/services
```

### Get Service Details
```
GET /mesh/services/:service_id
```

Returns full service info + health + routing stats.

### Register Service
```
POST /mesh/services
Body: RegisterServiceRequest (see Step 2)
```

### Unregister Service
```
DELETE /mesh/services/:service_id
```

### Get Service Health
```
GET /mesh/services/:service_id/health
```

### Get Service Latency
```
GET /mesh/services/:service_id/latency
```

---

## Partner Onboarding Flow

**1. Partner provides Docker image**

```bash
# Partner: "Here's our API"
docker push registry.yourplatform.com/partner-api:v1.0
```

**2. Platform deploys to regions**

```bash
# Your deployment script
./deploy_to_mesh.sh partner-api:v1.0 \
  --regions accra,lagos,nairobi \
  --health-path /health \
  --port 8080
```

**3. Auto-register in mesh**

```bash
# Your automation
curl -X POST http://gateway:8080/mesh/services -d '{
  "id": "partner-api",
  "name": "Partner API",
  "category": "payments",
  "endpoints": [...],  # Auto-generated from deployment
  "deployment": {
    "method": "runner",
    "runner_version": "1.0.0",
    "image": "registry.yourplatform.com/partner-api:v1.0",
    "version": "v1.0"
  }
}'
```

**4. Tenants can now use it**

```yaml
# Any tenant can now add:
sources:
  - id: partner
    type: servicemesh
    service: partner-api
    path: /v1/endpoint
```

**Partner gets:**
- Multi-region presence (your infra)
- Optimized kernel/network (your stack)
- Distribution to all tenants (your platform)

**Tenants get:**
- Low-latency access (< 15ms typically)
- Automatic failover (health-based)
- Zero config (one line)

---

## Monitoring & Debugging

### Check Overall Mesh Health

```bash
# List all services
curl http://localhost:8080/mesh/services

# For each service, check health
for service in $(curl -s http://localhost:8080/mesh/services | jq -r '.services[].id'); do
  echo "=== $service ==="
  curl -s http://localhost:8080/mesh/services/$service/health | jq '.stats'
done
```

### Find Slow Endpoints

```bash
# Get latency for all services
curl http://localhost:8080/mesh/services/hubtel-payments/latency | \
  jq '.endpoints[] | select(.stats.p95_ms > 100)'
```

### Watch Routing Decisions

```bash
# Check gateway logs
tail -f /var/log/spacecloud/gateway.log | grep "Routed to endpoint"
```

Example log:
```
2025-01-24T09:30:15Z DEBUG Routed to endpoint service=hubtel-payments
  endpoint_id=hubtel-accra-1 region=accra latency_p50=12ms
```

---

## Next Steps

**Phase 2: Runner Integration (Coming Soon)**
- Deploy services via SpaceCloud runner
- Auto-register on deployment
- Multi-region orchestration
- One-command deploys

**Phase 3: Advanced Features**
- Circuit breakers
- Rate limiting per endpoint
- Custom health check logic
- Metrics export to Prometheus
- Grafana dashboards

---

## Troubleshooting

### Service not routing

**Check registration:**
```bash
curl http://localhost:8080/mesh/services/your-service
```

If 404: Service not registered. Register it first.

**Check health:**
```bash
curl http://localhost:8080/mesh/services/your-service/health
```

If all unhealthy: Check endpoint URLs are accessible from gateway.

### Always routes to same endpoint

**Check latencies:**
```bash
curl http://localhost:8080/mesh/services/your-service/latency
```

If one endpoint significantly faster: That's correct! It's choosing the fastest.

To force different routing, use `region_affinity` or `round_robin`.

### Endpoint marked unhealthy but it's up

**Check health endpoint:**
```bash
# From gateway node
curl https://your-endpoint/health
```

Ensure:
- Returns 200 status
- Responds within 5s
- No certificate errors

---

## Support

- **Docs:** https://docs.spacecloud.io/mesh
- **Issues:** https://github.com/yourorg/spacecloud/issues
- **Slack:** #service-mesh

---

**The mesh is live and routing! 🚀**
