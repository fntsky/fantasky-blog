DIST_DIR := dist
BINARY := rs_blog
CHAMBER_DIR ?= ../formicary/tmp/chamber/rs_blog

.PHONY: all build build-frontend build-server copy-to-chamber run clean

all: build

build: build-frontend build-server

build-frontend:
	npm ci --prefix frontend
	npm run build --prefix frontend

build-server:
	cargo build --release
	mkdir -p $(DIST_DIR)
	cp target/release/$(BINARY) $(DIST_DIR)/$(BINARY)

copy-to-chamber: build
	mkdir -p "$(CHAMBER_DIR)"
	tar --exclude='./.git' --exclude='./target' --exclude='./tmp' --exclude='./frontend/node_modules' --exclude='./data/*.sqlite3*' -cf - . | tar -xf - -C "$(CHAMBER_DIR)"
	mkdir -p "$(CHAMBER_DIR)/data"
	if [ -f data/blog.sqlite3 ] && [ ! -f "$(CHAMBER_DIR)/data/blog.sqlite3" ]; then cp -p data/blog.sqlite3 "$(CHAMBER_DIR)/data/blog.sqlite3"; fi

run: build
	./start.sh

clean:
	cargo clean
	rm -rf $(DIST_DIR) frontend/dist
