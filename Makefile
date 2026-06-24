.PHONY: help dev build install clean test lint fmt check

# Default target
help:
	@echo "FileSync - Build and Run Targets"
	@echo ""
	@echo "Development:"
	@echo "  make dev         - Run app in development mode (hot reload)"
	@echo "  make build       - Build optimized release binary and .app bundle"
	@echo ""
	@echo "Installation:"
	@echo "  make install     - Install built .app to /Applications"
	@echo ""
	@echo "Testing & Linting:"
	@echo "  make test        - Run all Rust tests"
	@echo "  make lint        - Run clippy (Rust linter)"
	@echo "  make fmt         - Format Rust and UI code"
	@echo "  make check       - Run clippy + fmt check (CI mode)"
	@echo ""
	@echo "Utilities:"
	@echo "  make clean       - Remove build artifacts"
	@echo "  make deps        - Install UI dependencies"

# Run in development mode (always from root)
dev:
	@echo "Starting FileSync in development mode..."
	cargo tauri dev

# Build optimized release
build:
	@echo "Building release version..."
	cargo tauri build
	@echo ""
	@echo "✓ Build complete!"
	@echo "  App bundle: src-tauri/target/release/bundle/macos/filesync.app"

# Install to /Applications
install: build
	@echo "Installing to /Applications..."
	cp -r src-tauri/target/release/bundle/macos/filesync.app /Applications/
	@echo "✓ Installed to /Applications/filesync.app"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf ui/node_modules ui/dist
	@echo "✓ Clean complete"

# Install UI dependencies (rarely needed - handled by tauri commands)
deps:
	@echo "Installing UI dependencies..."
	cd ui && pnpm install
	@echo "✓ Dependencies installed"

# Run all tests
test:
	@echo "Running Rust tests..."
	cargo test --all --color=always
	@echo "✓ All tests passed"

# Run clippy linter
lint:
	@echo "Running clippy..."
	cargo clippy --all-targets --color=always
	@echo "✓ Clippy checks passed"

# Format code
fmt:
	@echo "Formatting Rust code..."
	cargo fmt --all
	@echo "Formatting UI code..."
	cd ui && pnpm prettier --write src
	@echo "✓ Formatting complete"

# Check formatting and linting (CI mode)
check:
	@echo "Checking format..."
	cargo fmt --all -- --check
	@echo "Running clippy..."
	cargo clippy --all-targets -- -D warnings
	@echo "✓ All checks passed"
