.PHONY: build

build:
	@echo "Build client ...."
	rm -rf dict/*
	cd web-app && pnpm install && pnpm build && cd ..
	cp -r web-app/dist /

	@echo "Installing Poetry dependencies..."
	poetry lock --no-update
	poetry install

	@echo "Activating Poetry shell..."
	poetry run pip install .

	@echo "Building Rust project..."
	poetry run cargo build --release

	@echo "Build complete!"
