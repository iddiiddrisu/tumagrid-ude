# UDE - Universal Developer Engine

A high-performance Backend-as-a-Service (BaaS) platform written in Rust, featuring a powerful API orchestration engine and service mesh capabilities. UDE provides developers with a universal engine to build, deploy, and scale applications with ease.

> **⚠️ BETA SOFTWARE**: UDE is currently in beta and under active development. It is suitable for development, testing, and internal projects, but **NOT recommended for production use** without additional security hardening. See [Known Limitations](#️-known-limitations-beta-release) for details.

## Features

- ✅ **Multi-Database Support**: PostgreSQL, MySQL, MongoDB (SQL Server coming soon)
- ✅ **REST APIs**: Full CRUD operations via REST endpoints
- ⚠️ **Authentication & Authorization**: JWT authentication ✅ | Authorization rules ⚠️ (basic only, advanced rules not implemented)
- ⭐ **API Orchestration Engine**: Compose data from multiple sources (databases, REST APIs, GraphQL, serverless functions) in a single request with automatic parallelization and cross-source joins
- ⭐ **Service Mesh**: Smart routing for managed services with health checking, latency tracking, and region-aware routing
- ✅ **Real-time Ready**: Async/await architecture built on Tokio
- ✅ **Type-Safe**: Leveraging Rust's type system for compile-time guarantees
- ✅ **High Performance**: Built with Rust for maximum performance and efficiency
- ✅ **Observability**: Full OpenTelemetry integration with traces, metrics, and logs
- 🚧 **GraphQL Support**: Coming soon
- 🚧 **Event System**: Intent-Stage-Complete pattern (coming soon)
- 🚧 **File Storage**: S3, GCS, Local (coming soon)

## Project Structure

```
ude/
├── crates/
│   ├── core/           # Core types, traits, and error handling
│   ├── modules/        # Business logic modules (CRUD, Auth, etc.)
│   ├── gateway/        # HTTP server and API handlers
│   ├── managers/       # Cluster coordination (TODO)
│   └── utils/          # Shared utilities (TODO)
├── config.example.yaml # Example configuration
└── Cargo.toml         # Workspace configuration
```

## Quick Start

### Prerequisites

- Rust 1.70+ (2021 edition)
- PostgreSQL/MySQL/SQL Server (at least one)
- Redis (optional, for caching)

### Installation

1. Clone the repository:
```bash
git clone https://github.com/iddiiddrisu/tumagrid-ude.git
cd tumagrid-ude
```

2. Copy the example configuration:
```bash
cp config.example.yaml config.yaml
```

3. Edit `config.yaml` with your database credentials

4. Build the project:
```bash
cargo build --release
```

5. Run the gateway:
```bash
cargo run --bin gateway -- --config config.yaml
```

The server will start on `http://localhost:4122` by default.

## Configuration

### Environment Variables

- `CONFIG_PATH` - Path to configuration file
- `PORT` - Server port (default: 4122)
- `NODE_ID` - Unique node identifier
- `CLUSTER_ID` - Cluster identifier (required)
- `LOG_LEVEL` - Logging level (debug, info, warn, error)
- `LOG_FORMAT` - Log format (json, text)

### Configuration File

See `config.example.yaml` for a complete configuration reference.

Key sections:
- **projects**: Define your applications and their configurations
- **databaseConfigs**: Database connection settings
- **databaseRules**: Access control rules for collections/tables
- **auths**: JWT authentication configuration
- **eventingConfig**: Event triggers and webhooks
- **fileStoreConfig**: File storage backends

## API Usage

### CRUD Operations

All CRUD endpoints follow the pattern:
```
POST /v1/api/{project}/crud/{db_alias}/{collection}/{operation}
```

#### Create

```bash
curl -X POST http://localhost:4122/v1/api/myapp/crud/postgres/users/create \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "op": "one",
    "doc": {
      "name": "John Doe",
      "email": "john@example.com",
      "age": 30
    }
  }'
```

#### Read

```bash
curl -X POST http://localhost:4122/v1/api/myapp/crud/postgres/users/read \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "find": {
      "age": {">": 18}
    },
    "options": {
      "limit": 10,
      "sort": ["-age"],
      "select": {
        "name": 1,
        "email": 1
      }
    }
  }'
```

#### Update

```bash
curl -X POST http://localhost:4122/v1/api/myapp/crud/postgres/users/update \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "find": {
      "id": "user123"
    },
    "update": {
      "$set": {
        "age": 31
      }
    },
    "op": "set"
  }'
```

#### Delete

```bash
curl -X POST http://localhost:4122/v1/api/myapp/crud/postgres/users/delete \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "find": {
      "id": "user123"
    },
    "op": "one"
  }'
```

### Health Check

```bash
curl http://localhost:4122/v1/api/health
```

## Security Rules

UDE supports powerful, declarative security rules:

### Allow/Deny

```yaml
rule: allow  # or deny
```

### Authentication Required

```yaml
rule: authenticated
```

### Field Matching

```yaml
rule: match
type: "=="
f1: "auth.id"        # From JWT token
f2: "args.find.id"   # From request
```

### Complex Logic

```yaml
rule: and
clauses:
  - rule: authenticated
  - rule: match
    type: "=="
    f1: "auth.role"
    f2: "admin"
```

## Architecture

### Core Principles

1. **Trait-Based**: All modules implement traits for flexible dependency injection
2. **Async/Await**: Built on Tokio for high-performance async I/O
3. **Type-Safe**: Newtypes and strong typing prevent common errors
4. **Lock-Free Reads**: Arc-swap for hot config reloading without locks
5. **Structured Concurrency**: Proper task lifecycle management

### Key Components

- **CrudModule**: Database abstraction layer with multi-driver support
- **AuthModule**: JWT authentication and rule-based authorization
- **AppState**: Application state with hot-reload capability
- **Query Builder**: Translates JSON queries to SQL

## Development

### Running Tests

```bash
cargo test
```

### Running with Debug Logs

```bash
RUST_LOG=debug cargo run --bin gateway -- --config config.yaml
```

### Code Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

## ⚠️ Known Limitations (Beta Release)

**IMPORTANT: UDE is currently in BETA. It is suitable for development, testing, and internal projects, but NOT recommended for production use without additional security hardening.**

### 🔴 Security Limitations
- **Authorization Not Fully Implemented**: CRUD endpoints have TODOs for authorization checks. Currently, basic rule matching works (allow, deny, authenticated, match, and, or), but advanced features are incomplete:
  - ❌ Query rules (nested query evaluation)
  - ❌ Webhook rules
  - ❌ Post-processing (field filtering, encryption)
- **Risk**: Without proper authorization, any authenticated user could potentially access any data
- **Recommendation**: Implement application-level authorization or use UDE behind a secure API gateway

### ⚠️ Testing & Quality
- **Test Coverage**: ~30% (needs improvement)
  - ✅ JWT handler has tests
  - ✅ Query builder has tests
  - ❌ Most modules lack comprehensive tests
  - ❌ No integration tests
  - ❌ No end-to-end tests
- **Recommendation**: Add thorough testing before using with critical data

### 📋 Missing Features
The following features are documented but not yet implemented:
- ❌ **GraphQL Support**: Schema generation, queries, mutations, subscriptions
- ❌ **Event System**: Intent-Stage-Complete pattern, background processing
- ❌ **File Storage**: S3, GCS, Local storage backends
- ❌ **Real-time**: WebSocket support, live queries
- ❌ **Schema Management**: Validation, inspection, type inference
- ❌ **Distributed Systems**: Redis clustering, leader election

### ✅ What Works Well
- ✅ Database CRUD (PostgreSQL, MySQL, MongoDB)
- ✅ JWT token creation and validation
- ✅ HTTP gateway with excellent performance
- ✅ OpenTelemetry observability (traces, metrics, logs)
- ⭐ **API Orchestration Engine** (fully functional - unique feature!)
- ⭐ **Service Mesh** (fully functional - unique feature!)
- ✅ Docker deployment
- ✅ Hot-reload configuration

### 📈 Production Readiness Estimate
- **Current Status**: Beta (40% feature complete)
- **Estimated time to Production (v1.0)**: 8-12 weeks with dedicated team
- **Estimated time to Stable Beta**: 2-3 weeks (security + testing)

## Roadmap

### Phase 1: Foundation ✅ (Complete)
- ✅ Core types and traits
- ✅ Error handling
- ✅ Configuration system
- ✅ PostgreSQL/MySQL support
- ✅ MongoDB driver (complete!)
- ✅ Basic CRUD operations
- ✅ HTTP server with Axum
- ✅ JWT authentication
- ✅ Basic authorization rules (allow, deny, authenticated, match, and, or)
- ✅ OpenTelemetry observability
- ✅ API Orchestration Engine (COMPLETE - unique feature!)
- ✅ Service Mesh (COMPLETE - unique feature!)

### Phase 2: Security & Stability 🔴 (Critical - Not Started)
**Timeline: 2-3 weeks**
- ❌ Implement authorization checks in CRUD handlers
- ❌ Complete advanced authorization rules (Query, Webhook)
- ❌ Add post-processing (field filtering, encryption)
- ❌ Integration tests
- ❌ End-to-end tests
- ❌ Security audit
- ❌ CI/CD pipeline setup

### Phase 3: Core Features 🚧 (Not Started)
**Timeline: 4-6 weeks**
- ❌ SQL Server support (awaiting SQLx 0.8)
- ❌ Schema validation
- ❌ Schema inspection
- ❌ GraphQL support (schema generation, queries, mutations)
- ❌ Event system (Intent-Stage-Complete pattern)
- ❌ Real-time subscriptions (WebSockets)
- ❌ File storage (S3, GCS, Local)

### Phase 4: Advanced Features ⏳ (Not Started)
**Timeline: 2-3 weeks**
- ❌ Nested query evaluation in auth rules
- ❌ Webhook rules
- ❌ Query batching
- ❌ Load testing and optimization
- ❌ Performance benchmarks
- ❌ API documentation (rustdoc)

### Phase 5: Distributed Systems ⏳ (Not Started)
**Timeline: 2-3 weeks**
- ❌ Redis pub/sub for clustering
- ❌ Configuration synchronization
- ❌ Leader election
- ❌ Kubernetes integration (Helm charts)
- ❌ Admin UI

## Performance

**Expected Performance Characteristics** (not yet benchmarked):
- **Language**: Rust provides zero-cost abstractions and no garbage collection
- **Runtime**: Tokio async runtime for efficient I/O
- **Compilation**: Optimized release builds with LTO and single codegen unit
- **Concurrency**: Lock-free reads with arc-swap for hot-reload

**Note**: Performance benchmarks and load testing have not yet been conducted. The above are theoretical benefits of the chosen technology stack.

**TODO**:
- [ ] Conduct load testing
- [ ] Establish performance baselines
- [ ] Create benchmark suite
- [ ] Compare against similar BaaS platforms

## Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Rust community for excellent crates (Tokio, Axum, SQLx, etc.)
- Open source BaaS projects for inspiration
- All contributors and users

## Support

- Issues: GitHub Issues
- Discussions: GitHub Discussions
- Documentation: See `RUST_DESIGN_DOCUMENT.md` for technical details
