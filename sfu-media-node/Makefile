# ==============================================================================
# Makefile for sfu-media-node (Rust Media Plane)
# ==============================================================================

.PHONY: all help build release check test test-unit test-lifecycle test-media test-observability test-refactoring test-capacity fmt fmt-check lint run docker-build docker-run clean

IMAGE_NAME ?= ekbatan/sfu-media-node
IMAGE_TAG ?= latest

help: ## Show this help message
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

all: fmt-check lint test release ## Run formatting check, clippy, all tests, and release build

build: ## Compile debug binary
	cargo build

release: ## Compile optimized release binary
	cargo build --release

check: ## Fast compilation check
	cargo check

test: ## Run all unit and integration tests
	cargo test

test-unit: ## Run unit tests only
	cargo test --lib

test-lifecycle: ## Run router and transport lifecycle tests
	cargo test --test lifecycle_tests

test-media: ## Run media produce and consume tests
	cargo test --test media_tests

test-observability: ## Run observability and metrics tests
	cargo test --test observability_tests

test-refactoring: ## Run refactoring verification tests
	cargo test --test refactoring_verification_tests

test-capacity: ## Run multi-worker spillover and capacity metrics tests
	cargo test --test multi_worker_capacity_tests

fmt: ## Format Rust codebase using rustfmt
	cargo fmt

fmt-check: ## Check code formatting without applying changes
	cargo fmt --check

lint: ## Run Clippy linter with warnings denied
	cargo clippy --all-targets -- -D warnings

run: ## Run the SFU media node locally
	cargo run

docker-build: ## Build multi-stage production Docker image
	docker build -t $(IMAGE_NAME):$(IMAGE_TAG) .

docker-run: ## Run Docker container locally with host networking
	docker run --rm -it --network host \
		-e MEDIASOUP_LISTEN_IP=0.0.0.0 \
		-e MEDIASOUP_ANNOUNCED_IP=127.0.0.1 \
		-e RTC_MIN_PORT=40000 \
		-e RTC_MAX_PORT=49999 \
		-e GRPC_PORT=50051 \
		-e OBS_LOG_LEVEL=info \
		$(IMAGE_NAME):$(IMAGE_TAG)

clean: ## Clean build target directory
	cargo clean
