APP_NAME     ?= signaling-coordinator
CMD_DIR      ?= ./cmd/coordinator
BIN_DIR      ?= ./bin
BINARY       ?= $(BIN_DIR)/$(APP_NAME)
GO           ?= go
DOCKER       ?= docker
IMAGE_NAME   ?= ekbatan-room/signaling-coordinator
IMAGE_TAG    ?= latest

GIT_COMMIT   ?= $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_TIME   ?= $(shell date -u +'%Y-%m-%dT%H:%M:%SZ')
VERSION      ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "v0.1.0")
LDFLAGS      := -s -w -X main.version=$(VERSION) -X main.gitCommit=$(GIT_COMMIT) -X main.buildTime=$(BUILD_TIME)

# Default target
.DEFAULT_GOAL := help

.PHONY: help
help: ## Display this help screen
	@awk 'BEGIN {FS = ":.*##"; printf "\n\033[1mUsage:\033[0m\n  make \033[36m<target>\033[0m\n\n\033[1mTargets:\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

.PHONY: all
all: vet test build ## Run vet, tests, and build binary

.PHONY: build
build: ## Build the static binary for the host platform
	@echo "==> Building $(APP_NAME) binary..."
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=0 $(GO) build -trimpath -ldflags="$(LDFLAGS)" -o $(BINARY) $(CMD_DIR)
	@echo "==> Built: $(BINARY)"

.PHONY: run
run: ## Run coordinator locally
	@echo "==> Running $(APP_NAME)..."
	$(GO) run $(CMD_DIR)

.PHONY: test
test: ## Run unit and integration tests
	@echo "==> Running tests..."
	$(GO) test -v -count=1 ./...

.PHONY: test-race
test-race: ## Run all tests with race condition detector
	@echo "==> Running tests with race detector (-race)..."
	$(GO) test -v -race -count=1 ./...

.PHONY: test-coverage
test-coverage: ## Run tests with code coverage report
	@echo "==> Running tests with coverage profile..."
	$(GO) test -race -coverprofile=coverage.out -covermode=atomic ./...
	$(GO) tool cover -func=coverage.out

.PHONY: vet
vet: ## Run go vet across all packages
	@echo "==> Running go vet..."
	$(GO) vet ./...

.PHONY: fmt
fmt: ## Format Go source code
	@echo "==> Formatting code..."
	@gofmt -s -w .

.PHONY: tidy
tidy: ## Tidy and verify go.mod and go.sum dependencies
	@echo "==> Tidying dependencies..."
	$(GO) mod tidy
	$(GO) mod verify

.PHONY: proto
proto: ## Regenerate Go protobuf stubs from proto definitions
	@echo "==> Generating protobuf stubs..."
	@command -v protoc >/dev/null 2>&1 || { echo >&2 "Error: protoc is required but not installed."; exit 1; }
	@mkdir -p gen/proto/sfu/v1
	protoc --go_out=. --go_opt=module=ekbatan-room/signaling-coordinator \
	       --go-grpc_out=. --go-grpc_opt=module=ekbatan-room/signaling-coordinator \
	       proto/sfu/v1/media_engine.proto
	@echo "==> Protobuf stubs regenerated in gen/proto/sfu/v1"

.PHONY: docker-build
docker-build: ## Build multi-stage production Docker container
	@echo "==> Building Docker image $(IMAGE_NAME):$(IMAGE_TAG)..."
	$(DOCKER) build \
		--build-arg VERSION=$(VERSION) \
		--build-arg GIT_COMMIT=$(GIT_COMMIT) \
		--build-arg BUILD_TIME=$(BUILD_TIME) \
		-t $(IMAGE_NAME):$(IMAGE_TAG) .

.PHONY: docker-run
docker-run: ## Run Docker container locally with default ports (7880)
	@echo "==> Running Docker container $(IMAGE_NAME):$(IMAGE_TAG)..."
	$(DOCKER) run --rm -it \
		-p 7880:7880 \
		-e PORT=7880 \
		-e SFU_GRPC_ADDR=host.docker.internal:50051 \
		-e OBS_LOG_LEVEL=info \
		-e OBS_LOG_FORMAT=json \
		-e OBS_METRICS_ENABLED=true \
		$(IMAGE_NAME):$(IMAGE_TAG)

.PHONY: clean
clean: ## Clean build artifacts and test coverage profiles
	@echo "==> Cleaning artifacts..."
	@rm -rf $(BIN_DIR) coverage.out coverage.html
	@echo "==> Cleaned."
