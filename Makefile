# Nexus — developer convenience Makefile
#
# Default target: `make` or `make dev` starts the backend + frontend together.
# Run `make help` to see everything available.

.DEFAULT_GOAL := dev

# ---------------------------------------------------------------------------
# Config — override with e.g. `make dev BACKEND_PORT=9090`
# ---------------------------------------------------------------------------
BACKEND_PORT ?= 8080
FRONTEND_PORT ?= 3001
NEXUS_DATA_DIR ?= $(HOME)/.nexus

# Colours (only used when stdout is a tty)
YELLOW := \033[1;33m
GREEN  := \033[0;32m
RED    := \033[0;31m
BOLD   := \033[1m
NC     := \033[0m

.PHONY: dev install test reset doctor help \
        dev-backend dev-frontend build fmt clippy

# ---------------------------------------------------------------------------
# dev — run backend + frontend together
# ---------------------------------------------------------------------------
dev:
	@echo "$(BOLD)$(GREEN)==>$(NC) starting nexus-http on :$(BACKEND_PORT) + web on :$(FRONTEND_PORT)"
	@echo "$(YELLOW)Tip:$(NC) Ctrl-C stops both. Logs interleave — prefix [be]/[fe] identifies source."
	@trap 'kill 0' EXIT INT TERM; \
	  ( NEXUS_PORT=$(BACKEND_PORT) cargo run -p nexus-http 2>&1 | sed -u 's/^/[be] /' ) & \
	  ( cd web && npm run dev 2>&1 | sed -u 's/^/[fe] /' ) & \
	  wait

dev-backend:
	@echo "$(BOLD)$(GREEN)==>$(NC) starting nexus-http on :$(BACKEND_PORT)"
	NEXUS_PORT=$(BACKEND_PORT) cargo run -p nexus-http

dev-frontend:
	@echo "$(BOLD)$(GREEN)==>$(NC) starting web on :$(FRONTEND_PORT)"
	cd web && npm run dev

# ---------------------------------------------------------------------------
# install — build Rust release artefacts + install web deps
# ---------------------------------------------------------------------------
install:
	@echo "$(BOLD)$(GREEN)==>$(NC) building Rust workspace (release)"
	cargo build --release
	@echo "$(BOLD)$(GREEN)==>$(NC) installing web dependencies"
	cd web && npm install
	@echo "$(BOLD)$(GREEN)done.$(NC) Run '$(BOLD)make dev$(NC)' to start everything."

# ---------------------------------------------------------------------------
# build — alias for cargo build (debug)
# ---------------------------------------------------------------------------
build:
	cargo build

# ---------------------------------------------------------------------------
# test — run rust tests + web tests
# ---------------------------------------------------------------------------
test:
	@echo "$(BOLD)$(GREEN)==>$(NC) cargo test --workspace"
	cargo test --workspace
	@echo "$(BOLD)$(GREEN)==>$(NC) web tests"
	cd web && npm test

# ---------------------------------------------------------------------------
# reset — wipe the local SQLite DB so you can start fresh
# ---------------------------------------------------------------------------
reset:
	@echo "$(BOLD)$(RED)!!$(NC) This will delete $(NEXUS_DATA_DIR)/nexus.db"
	@printf "Type $(BOLD)yes$(NC) to confirm: " && read ans && [ "$$ans" = "yes" ] || ( echo "aborted."; exit 1 )
	@rm -f "$(NEXUS_DATA_DIR)/nexus.db" "$(NEXUS_DATA_DIR)/nexus.db-wal" "$(NEXUS_DATA_DIR)/nexus.db-shm"
	@echo "$(GREEN)removed $(NEXUS_DATA_DIR)/nexus.db$(NC)"

# ---------------------------------------------------------------------------
# doctor — check that required toolchains are present
# ---------------------------------------------------------------------------
doctor:
	@echo "$(BOLD)$(GREEN)==>$(NC) nexus doctor"
	@printf "  rust:   " ; if command -v rustc >/dev/null 2>&1; then rustc --version; else printf "$(RED)missing$(NC) — install via https://rustup.rs\n"; fi
	@printf "  cargo:  " ; if command -v cargo >/dev/null 2>&1; then cargo --version; else printf "$(RED)missing$(NC) — install via https://rustup.rs\n"; fi
	@printf "  node:   " ; if command -v node >/dev/null 2>&1; then node --version;  else printf "$(RED)missing$(NC) — install via https://nodejs.org (>= 20)\n"; fi
	@printf "  npm:    " ; if command -v npm >/dev/null 2>&1;  then npm --version;   else printf "$(RED)missing$(NC) — ships with node\n"; fi
	@printf "  docker: " ; if command -v docker >/dev/null 2>&1; then docker --version; else printf "$(YELLOW)missing$(NC) (optional) — only needed for 'docker compose up'\n"; fi
	@printf "  data:   $(NEXUS_DATA_DIR)"; if [ -d "$(NEXUS_DATA_DIR)" ]; then echo " ($(GREEN)exists$(NC))"; else echo " ($(YELLOW)will be created on first run$(NC))"; fi

# ---------------------------------------------------------------------------
# fmt / clippy — code-quality shortcuts
# ---------------------------------------------------------------------------
fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# ---------------------------------------------------------------------------
# help — print all targets
# ---------------------------------------------------------------------------
help:
	@echo "$(BOLD)Nexus Makefile$(NC)"
	@echo ""
	@echo "  $(BOLD)make dev$(NC)           start backend + frontend together (default)"
	@echo "  $(BOLD)make dev-backend$(NC)   start only the Rust backend (port $(BACKEND_PORT))"
	@echo "  $(BOLD)make dev-frontend$(NC)  start only the Next.js frontend (port $(FRONTEND_PORT))"
	@echo "  $(BOLD)make install$(NC)       cargo build --release + npm install"
	@echo "  $(BOLD)make build$(NC)         cargo build (debug)"
	@echo "  $(BOLD)make test$(NC)          cargo test --workspace + web tests"
	@echo "  $(BOLD)make reset$(NC)         wipe $(NEXUS_DATA_DIR)/nexus.db (with confirmation)"
	@echo "  $(BOLD)make doctor$(NC)        check for rust / node / docker"
	@echo "  $(BOLD)make fmt$(NC)           cargo fmt --all"
	@echo "  $(BOLD)make clippy$(NC)        cargo clippy --all -- -D warnings"
	@echo "  $(BOLD)make help$(NC)          this message"
	@echo ""
	@echo "  Env: BACKEND_PORT=$(BACKEND_PORT) FRONTEND_PORT=$(FRONTEND_PORT) NEXUS_DATA_DIR=$(NEXUS_DATA_DIR)"
