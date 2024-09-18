.PHONY: build

build:
	@echo "Installing Poetry dependencies..."
	poetry install

	@echo "Activating Poetry shell..."
	poetry run pip install .

	@echo "Building Rust project..."
	poetry run cargo build

	@echo "Build complete!"
