SHELL        := /usr/bin/env bash
ROOT_DIR     := $(shell pwd)
BIN_DIR      := $(ROOT_DIR)/bin
LK_BIN       := $(BIN_DIR)/lk
SCENARIO_DIR := $(ROOT_DIR)/scenarios

.DEFAULT_GOAL := help

.PHONY: help
help: ## نمایش دستورات در دسترس
	@awk 'BEGIN {FS = ":.*##"; printf "\n\033[1mUsage:\033[0m\n  make \033[36m<target>\033[0m\n\n\033[1mTargets:\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

.PHONY: install-lk
install-lk: ## دانلود و نصب ابزار رسمی livekit-cli در پوشه bin
	@if [ ! -f "$(LK_BIN)" ]; then \
		echo "==> Downloading official livekit-cli (lk)..."; \
		mkdir -p $(BIN_DIR); \
		LK_INSTALL_PATH=$(BIN_DIR) bash <(curl -sSL https://get.livekit.io/cli); \
	else \
		echo "==> livekit-cli already installed at: $(LK_BIN)"; \
		$(LK_BIN) --version; \
	fi

.PHONY: run-baseline
run-baseline: install-lk ## گیت ۱: اجرای سناریوی پایه اعتبارسنجی (۱ ناشر، ۱۰ شنونده)
	@echo "==> Running Gate 1: Baseline Sanity Test..."
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-01-baseline.yaml

.PHONY: run-broadcast
run-broadcast: install-lk ## گیت ۲: اجرای سناریوی پخش زنده (۱ ناشر، ۵۰ شنونده با Simulcast)
	@echo "==> Running Gate 2: Broadcast Fan-Out Test..."
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-02-broadcast.yaml

.PHONY: run-conference
run-conference: install-lk ## گیت ۳: اجرای سناریوی جلسه گروهی تعاملی (۱۰ ناشر، ۱۰ شنونده)
	@echo "==> Running Gate 3: Group Conference Mesh Test..."
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-03-group-conference.yaml

.PHONY: run-spillover
run-spillover: install-lk ## گیت ۴: اجرای سناریوی اشباع و سرریز ورکرها در مدیاسوپ
	@echo "==> Running Gate 4: Worker Saturation & Spillover Test..."
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-04-worker-spillover.yaml

.PHONY: run-all
run-all: install-lk ## اجرای ترتیبی و گیت‌بندی شده تمام سناریوها (توقف در صورت شکست پیش‌نیاز)
	@echo "==> [START] Executing Sequential Gated Load Test Suite..."
	@echo "--- STEP 1/4: Baseline Sanity ---"
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-01-baseline.yaml || { echo "==> [FAIL] Gate 1 Failed! Gates 2-4 are BLOCKED."; exit 1; }
	@echo "--- STEP 2/4: Broadcast Fan-Out ---"
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-02-broadcast.yaml || { echo "==> [FAIL] Gate 2 Failed! Gates 3-4 are BLOCKED."; exit 1; }
	@echo "--- STEP 3/4: Group Conference ---"
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-03-group-conference.yaml || { echo "==> [FAIL] Gate 3 Failed! Gate 4 is BLOCKED."; exit 1; }
	@echo "--- STEP 4/4: Saturation & Spillover ---"
	@./scripts/run_scenario.sh $(SCENARIO_DIR)/gate-04-worker-spillover.yaml || { echo "==> [FAIL] Gate 4 Failed!"; exit 1; }
	@echo "==> [ALL GATES PASSED] Full stack load test succeeded!"

.PHONY: clean
clean: ## پاکسازی نتایج و لاگ‌های تست قبلی
	@rm -rf $(ROOT_DIR)/test-results/*
	@echo "==> test-results cleaned."
