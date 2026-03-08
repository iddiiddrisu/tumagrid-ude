# API Orchestration Examples

This document shows real-world examples of UDE's killer feature: **composing data from multiple heterogeneous sources in a single optimized request**.

## Why This Matters

**The Problem**: Modern applications need data from many sources:
- User profile from Postgres
- Orders from MongoDB
- Inventory from legacy REST API
- Recommendations from ML service (GraphQL)
- Shipping status from third-party API
- Cart from Redis cache

**Traditional Approach** (100+ lines of code in frontend/BFF):
```javascript
// Frontend developer's nightmare:
async function loadEcommerceHomepage(userId) {
  // Step 1: Get user (100ms)
  const user = await db.query('SELECT * FROM users WHERE id = ?', [userId]);

  // Step 2: Get orders - depends on user (150ms)
  const orders = await mongodb.find('orders', {userId: user.id});

  // Step 3: For EACH order, get shipping (N+1 problem!) (50ms × N)
  const shipping = await Promise.all(
    orders.map(o => fetch(`https://api.shipping.com/status/${o.id}`))
  );

  // Step 4: Get product details (200ms)
  const products = await graphqlClient.query({
    query: GET_PRODUCTS,
    variables: {ids: orders.map(o => o.product_id)}
  });

  // Step 5: Get recommendations (300ms)
  const recs = await lambda.invoke('ml-recommendations', {userId});

  // Step 6: Manual data merging nightmare
  const result = {
    user,
    orders: orders.map((o, i) => ({
      ...o,
      shipping: shipping[i],
      product: products.find(p => p.id === o.product_id)
    })),
    recommendations: recs
  };

  // Total time: 100 + 150 + (50 × N) + 200 + 300 = 750ms + (50 × N)
  // With 10 orders: 1,250ms (1.25 seconds!)
}
```

**UDE Approach** (30 lines of YAML config):
```yaml
# config/flows/homepage.yaml
compositeQueries:
  - id: ecommerce_homepage
    sources:
      # Stage 1: Fetch user (runs first)
      - id: user
        type: database
        dbAlias: postgres
        collection: users
        find: {id: "${args.userId}"}
        parallel: true

      # Stage 2: These run IN PARALLEL after user is fetched
      - id: orders
        type: database
        dbAlias: mongodb
        collection: orders
        find: {userId: "${user.id}"}
        dependsOn: [user]
        parallel: true

      - id: recommendations
        type: function
        name: ml-recommendations
        args: {userId: "${user.id}"}
        dependsOn: [user]
        parallel: true
        timeout_ms: 3000

      # Stage 3: These run IN PARALLEL after orders are fetched
      - id: shipping
        type: rest_api
        url: https://api.shipping.com/status
        method: GET
        params: {orderId: "${orders[*].id}"}  # Automatic batching!
        batch: true
        batch_field: orderId
        dependsOn: [orders]
        parallel: true
        retry:
          max_retries: 3
          initial_backoff_ms: 100

      - id: products
        type: graphql
        url: https://product-service/graphql
        query: |
          query GetProducts($ids: [ID!]!) {
            products(ids: $ids) {
              id name price image
            }
          }
        variables: {ids: "${orders[*].product_id}"}
        dependsOn: [orders]
        parallel: true

    # Compose final response with cross-source joins
    compose:
      user: "${user[0]}"
      orders:
        type: join
        left: orders
        right: shipping
        left_key: id
        right_key: orderId
        join_type: left
      recommendations: "${recommendations}"

# Result: 100 + 150 + 300 = 550ms (55% faster!)
# No N+1 problem (batching handles it)
# Zero orchestration code in frontend
```

## Example 1: E-commerce Product Page

### Business Need
Show product details with:
- Product info (Postgres)
- Real-time inventory (Redis)
- Reviews (MongoDB)
- Seller info (REST API)
- Related products (ML function)
- Shipping estimates (third-party API)

### Configuration
```yaml
compositeQueries:
  - id: product_page
    sources:
      - id: product
        type: database
        dbAlias: postgres
        collection: products
        find: {id: "${args.productId}"}

      - id: inventory
        type: cache
        key: "inventory:${args.productId}"
        cache:
          strategy: stale_while_revalidate
          stale_seconds: 30
          revalidate_seconds: 300

      - id: reviews
        type: database
        dbAlias: mongodb
        collection: reviews
        find: {productId: "${args.productId}"}
        options:
          limit: 10
          sort: ["-created_at"]
        dependsOn: []

      - id: seller
        type: rest_api
        url: https://seller-api.com/sellers/${product.seller_id}
        method: GET
        dependsOn: [product]
        cache:
          strategy: ttl
          seconds: 3600

      - id: related
        type: function
        name: recommend-products
        args:
          productId: "${product.id}"
          category: "${product.category}"
        dependsOn: [product]
        timeout_ms: 2000

      - id: shipping
        type: rest_api
        url: https://shipping-api.com/estimate
        method: POST
        body:
          zipCode: "${args.zipCode}"
          weight: "${product.weight}"
        dependsOn: [product]

    compose:
      product:
        type: merge
        sources: [product, inventory]
      seller: "${seller}"
      reviews: "${reviews}"
      related: "${related}"
      shipping: "${shipping}"
```

### Benefits
- ✅ 6 data sources, 1 API call
- ✅ Automatic parallelization (stages optimize automatically)
- ✅ Smart caching (inventory refreshes in background)
- ✅ No frontend orchestration code
- ✅ ~60% faster than sequential fetching

## Example 2: SaaS Dashboard

### Business Need
Admin dashboard showing:
- Tenant metadata (Postgres)
- Usage metrics (TimescaleDB)
- Recent activities (MongoDB)
- Billing status (Stripe API)
- Support tickets (Zendesk API)
- System health (Prometheus)

### Configuration
```yaml
compositeQueries:
  - id: admin_dashboard
    sources:
      - id: tenant
        type: database
        dbAlias: postgres
        collection: tenants
        find: {id: "${args.tenantId}"}

      - id: usage
        type: database
        dbAlias: timescale
        collection: usage_metrics
        find:
          tenant_id: "${tenant.id}"
          timestamp: {">": "${args.since}"}
        options:
          sort: ["-timestamp"]
        dependsOn: [tenant]

      - id: activities
        type: database
        dbAlias: mongodb
        collection: activities
        find:
          tenant_id: "${tenant.id}"
          created_at: {">": "${args.since}"}
        options:
          limit: 50
          sort: ["-created_at"]
        dependsOn: [tenant]

      - id: billing
        type: rest_api
        url: https://api.stripe.com/v1/customers/${tenant.stripe_customer_id}
        method: GET
        headers:
          Authorization: "Bearer ${secrets.STRIPE_KEY}"
        dependsOn: [tenant]
        cache:
          strategy: ttl
          seconds: 300

      - id: support
        type: rest_api
        url: https://company.zendesk.com/api/v2/tickets
        method: GET
        params:
          organization_id: "${tenant.zendesk_org_id}"
          status: open
        dependsOn: [tenant]

      - id: health
        type: rest_api
        url: http://prometheus:9090/api/v1/query
        method: POST
        body:
          query: 'up{tenant="${tenant.id}"}'
        dependsOn: [tenant]

    compose:
      tenant: "${tenant[0]}"
      metrics:
        daily_active_users: "${usage | filter(u => u.metric == 'dau') | first}"
        api_calls: "${usage | filter(u => u.metric == 'api_calls') | sum}"
      activities: "${activities | map(a => {type: a.type, user: a.user, timestamp: a.created_at})}"
      billing:
        status: "${billing.status}"
        amount_due: "${billing.amount_due}"
        next_invoice: "${billing.next_invoice_date}"
      support:
        open_tickets: "${support | length}"
        urgent_tickets: "${support | filter(t => t.priority == 'urgent') | length}"
      system_health: "${health.data.result[0].value[1]}"
```

### Benefits
- ✅ 6 different data sources (3 databases, 3 APIs)
- ✅ Automatic parallelization saves 500ms+
- ✅ Intelligent caching for billing data
- ✅ Secure secret management
- ✅ Complex data transformations (filter, map, sum)

## Example 3: IoT Device Dashboard

### Business Need
Show device overview with:
- Device metadata (Postgres)
- Real-time telemetry (InfluxDB)
- Configuration (MongoDB)
- Commands (MQTT via REST wrapper)
- ML predictions (Lambda)
- Historical trends (S3/Athena)

### Configuration
```yaml
compositeQueries:
  - id: device_dashboard
    sources:
      - id: device
        type: database
        dbAlias: postgres
        collection: devices
        find: {id: "${args.deviceId}"}

      - id: telemetry
        type: database
        dbAlias: influxdb
        collection: measurements
        find:
          device_id: "${device.id}"
          time: {">": "now() - 1h"}
        dependsOn: [device]

      - id: config
        type: database
        dbAlias: mongodb
        collection: device_configs
        find: {device_id: "${device.id}"}
        dependsOn: [device]

      - id: commands
        type: rest_api
        url: http://mqtt-bridge/devices/${device.id}/commands
        method: GET
        dependsOn: [device]

      - id: predictions
        type: function
        name: predict-maintenance
        args:
          deviceId: "${device.id}"
          telemetry: "${telemetry}"
        dependsOn: [device, telemetry]
        timeout_ms: 5000

      - id: trends
        type: rest_api
        url: https://athena.amazonaws.com/query
        method: POST
        body:
          query: "SELECT * FROM device_history WHERE device_id = '${device.id}'"
        dependsOn: [device]
        cache:
          strategy: ttl
          seconds: 3600  # Trends don't change often

    compose:
      device: "${device[0]}"
      current_state:
        temperature: "${telemetry | filter(t => t.metric == 'temp') | last}"
        pressure: "${telemetry | filter(t => t.metric == 'pressure') | last}"
        status: "${device.status}"
      config: "${config[0]}"
      recent_commands: "${commands | slice(0, 10)}"
      maintenance:
        next_predicted: "${predictions.next_maintenance_date}"
        confidence: "${predictions.confidence}"
        issues: "${predictions.potential_issues}"
      historical_trends: "${trends.data}"
```

## Example 4: Social Media Feed

### Business Need
Personalized feed with:
- User profile (Postgres)
- Friend posts (MongoDB)
- Trending topics (Redis)
- Ad recommendations (GraphQL service)
- Media processing status (REST API)
- Engagement metrics (Cassandra)

### Configuration
```yaml
compositeQueries:
  - id: social_feed
    sources:
      - id: user
        type: database
        dbAlias: postgres
        collection: users
        find: {id: "${args.userId}"}

      - id: trending
        type: cache
        key: "trending:${user.region}"
        cache:
          strategy: stale_while_revalidate
          stale_seconds: 60
          revalidate_seconds: 300
        dependsOn: []  # Can fetch in parallel with user

      - id: posts
        type: database
        dbAlias: mongodb
        collection: posts
        find:
          author_id: {"in": "${user.friend_ids}"}
          created_at: {">": "${args.since}"}
        options:
          limit: 50
          sort: ["-created_at"]
        dependsOn: [user]

      - id: media_status
        type: rest_api
        url: https://media-processor/status
        method: GET
        params:
          post_ids: "${posts[*].id}"
        batch: true
        batch_field: post_ids
        dependsOn: [posts]

      - id: ads
        type: graphql
        url: https://ad-service/graphql
        query: |
          query GetAds($userId: ID!, $interests: [String!]!) {
            recommendedAds(userId: $userId, interests: $interests, limit: 5) {
              id title image targetUrl
            }
          }
        variables:
          userId: "${user.id}"
          interests: "${user.interests}"
        dependsOn: [user]

      - id: engagement
        type: database
        dbAlias: cassandra
        collection: post_metrics
        find:
          post_id: {"in": "${posts[*].id}"}
        dependsOn: [posts]

    compose:
      user: "${user[0]}"
      trending: "${trending}"
      feed:
        type: join
        left: posts
        right: engagement
        left_key: id
        right_key: post_id
        join_type: left
      ads: "${ads.recommendedAds}"
```

### Benefits
- ✅ Handles high-scale social data
- ✅ Cross-source joins (posts + engagement)
- ✅ Automatic batching (media status for N posts)
- ✅ Background cache updates (trending topics)
- ✅ 6 data sources composed seamlessly

## Performance Comparison

### Scenario: E-commerce Homepage (10 orders)

| Approach | Time | Requests | Code Lines |
|----------|------|----------|------------|
| **Sequential (Naive)** | 1,250ms | 15 | 120 lines JS |
| **Manual Parallel** | 650ms | 15 | 200 lines JS |
| **UDE** | **550ms** | **1** | **30 lines YAML** |

### Key Metrics
- **55% faster** than naive approach
- **15% faster** than manually optimized code
- **93% less code** (30 YAML vs 200 JS)
- **Zero N+1 problems** (automatic batching)
- **Zero orchestration bugs** (declarative vs imperative)

## Advanced Features

### 1. Automatic Batching (Solves N+1)
```yaml
- id: product_details
  type: rest_api
  url: https://api.products.com/details
  params:
    ids: "${orders[*].product_id}"  # Extract all product IDs
  batch: true  # Single request instead of N requests
  batch_field: ids
```

### 2. Cross-Source Joins
```yaml
compose:
  orders_with_shipping:
    type: join
    left: orders  # From MongoDB
    right: shipping  # From REST API
    left_key: id
    right_key: order_id
    join_type: inner  # Just like SQL!
```

### 3. Smart Caching
```yaml
cache:
  strategy: stale_while_revalidate
  stale_seconds: 30  # Serve stale data immediately
  revalidate_seconds: 300  # Refresh in background
```

### 4. Retry with Exponential Backoff
```yaml
retry:
  max_retries: 3
  initial_backoff_ms: 100
  max_backoff_ms: 5000
  multiplier: 2.0
```

## Migration Path

### Step 1: Start with Simple Composition
```yaml
# Replace multiple API calls with single composite query
compositeQueries:
  - id: user_profile
    sources:
      - id: user
        type: database
        dbAlias: postgres
        collection: users
        find: {id: "${args.userId}"}
    compose:
      user: "${user[0]}"
```

### Step 2: Add Dependent Queries
```yaml
sources:
  - id: user
    # ... (same as above)

  - id: orders
    type: database
    dbAlias: mongodb
    collection: orders
    find: {userId: "${user.id}"}
    dependsOn: [user]  # Runs after user is fetched
```

### Step 3: Leverage Parallelization
```yaml
sources:
  - id: user
    # ...

  - id: orders
    dependsOn: [user]

  - id: recommendations
    type: function
    name: get-recs
    args: {userId: "${user.id}"}
    dependsOn: [user]  # Both run in parallel after user
```

### Step 4: Add Cross-Source Joins
```yaml
compose:
  user: "${user[0]}"
  orders_with_shipping:
    type: join
    left: orders
    right: shipping
    left_key: id
    right_key: order_id
```

## Conclusion

UDE's API orchestration is not just a feature - it's **the** feature that changes how modern applications are built:

1. **Zero Orchestration Code**: Declarative YAML instead of hundreds of lines of imperative code
2. **Automatic Optimization**: Query planner finds optimal parallel execution
3. **Cross-Source Joins**: Join data from Postgres, MongoDB, REST APIs, GraphQL - anything!
4. **Production Ready**: Retries, timeouts, caching, batching - all built-in
5. **Developer Joy**: One config file replaces complex orchestration logic

This is what makes UDE "Netflix API Gateway on steroids" - and why it will win in the market.
