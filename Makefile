.PHONY: build

build:
	@echo "Build client ...."
	cd web-app && pnpm build
	cp -r web-app/dist src/

	@echo "Installing Poetry dependencies..."
	poetry lock --no-update
	poetry install

	@echo "Activating Poetry shell..."
	poetry run pip install .

	@echo "Building Rust project..."
	poetry run cargo build --release

	@echo "Build complete!"
