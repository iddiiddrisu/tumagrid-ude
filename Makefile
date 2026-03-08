# UDE (Universal Developer Engine) - Makefile

.PHONY: help build run test clean dev docker-build docker-run

help:
	@echo "UDE (Universal Developer Engine) - Available Commands"
	@echo ""
	@echo "  make build        - Build release binary"
	@echo "  make dev          - Build and run in development mode"
	@echo "  make run          - Run the gateway"
	@echo "  make test         - Run all tests"
	@echo "  make test-watch   - Run tests in watch mode"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make fmt          - Format code"
	@echo "  make lint         - Run clippy linter"
	@echo "  make docker-build - Build Docker image"
	@echo "  make docker-run   - Run Docker container"

build:
	cargo build --release

dev:
	RUST_LOG=debug cargo run --bin gateway -- \
		--config config.yaml \
		--log-level debug \
		--log-format text

run:
	cargo run --release --bin gateway -- --config config.yaml

test:
	cargo test --all

test-watch:
	cargo watch -x test

clean:
	cargo clean
	rm -rf target/

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

check:
	cargo check --all-targets --all-features

docker-build:
	docker build -t ude:latest .

docker-run:
	docker run -p 4122:4122 \
		-v $(PWD)/config.yaml:/app/config.yaml \
		ude:latest

# Database helpers
db-postgres:
	docker run -d \
		--name ude-postgres \
		-e POSTGRES_PASSWORD=postgres \
		-e POSTGRES_DB=ude \
		-p 5432:5432 \
		postgres:15

db-mysql:
	docker run -d \
		--name ude-mysql \
		-e MYSQL_ROOT_PASSWORD=mysql \
		-e MYSQL_DATABASE=ude \
		-p 3306:3306 \
		mysql:8

db-redis:
	docker run -d \
		--name ude-redis \
		-p 6379:6379 \
		redis:7

db-stop:
	docker stop ude-postgres ude-mysql ude-redis || true
	docker rm ude-postgres ude-mysql ude-redis || true
