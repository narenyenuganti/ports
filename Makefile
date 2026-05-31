# Ports.app packaging Makefile
#
# Targets:
#   make app    - build the Rust daemon + Swift front-end and assemble a
#                 signed build/Ports.app bundle (default target)
#   make ports  - build only the universal (or arm64-only fallback) ports binary
#   make swift  - build only the Swift PortsBar executable
#   make bundle - assemble build/Ports.app from prebuilt binaries
#   make sign   - ad-hoc codesign build/Ports.app
#   make clean  - remove build/
#
# Notes:
# - CARGO_TARGET_DIR is honored from the environment (falls back to ./target).
# - The ports binary is built universal (arm64 + x86_64) when possible. If the
#   x86_64 cross-build fails, it falls back to an arm64-only binary and prints a
#   clear warning; the build still succeeds on Apple Silicon.

SHELL := /bin/bash

BUILD_DIR      := build
ASSETS_DIR     := assets
APP            := $(BUILD_DIR)/Ports.app
CONTENTS       := $(APP)/Contents
MACOS_DIR      := $(CONTENTS)/MacOS
RESOURCES_DIR  := $(CONTENTS)/Resources
INFO_PLIST     := $(CONTENTS)/Info.plist
APP_ICON       := $(ASSETS_DIR)/AppIcon.icns

BUNDLE_ID      := com.ports.app
APP_NAME       := Ports
SWIFT_EXE      := PortsBar
SHORT_VERSION  := 0.1.0
BUILD_VERSION  := 1
MIN_OS         := 14.0

CARGO_TARGET   := $${CARGO_TARGET_DIR:-target}

.PHONY: all app ports swift bundle sign clean

# Default target: full signed bundle.
all: app

app: bundle sign
	@echo "==> Ports.app ready: $(APP)"

# Build the universal ports binary (arm64 + x86_64), with arm64-only fallback.
ports:
	@echo "==> Building ports (release, aarch64-apple-darwin)"
	cargo build --release --target aarch64-apple-darwin
	@mkdir -p $(BUILD_DIR)
	@echo "==> Attempting ports cross-build (x86_64-apple-darwin)"
	@arm64_bin="$(CARGO_TARGET)/aarch64-apple-darwin/release/ports"; \
	x86_bin="$(CARGO_TARGET)/x86_64-apple-darwin/release/ports"; \
	if cargo build --release --target x86_64-apple-darwin; then \
		echo "==> lipo: creating universal binary (arm64 + x86_64)"; \
		lipo -create -output $(BUILD_DIR)/ports "$$arm64_bin" "$$x86_bin"; \
	else \
		echo "WARNING: x86_64 cross-build failed; falling back to arm64-only binary."; \
		echo "WARNING: build/Ports.app will be arm64-only (not universal)."; \
		cp "$$arm64_bin" $(BUILD_DIR)/ports; \
	fi
	@chmod +x $(BUILD_DIR)/ports
	@echo "==> ports binary architecture:"; lipo -archs $(BUILD_DIR)/ports

# Build the Swift PortsBar release executable.
swift:
	@echo "==> Building Swift PortsBar (release)"
	cd app && swift build -c release

# Assemble build/Ports.app from the built binaries.
bundle: ports swift
	@echo "==> Assembling $(APP)"
	@rm -rf "$(APP)"
	@mkdir -p "$(MACOS_DIR)" "$(RESOURCES_DIR)"
	@swift_bin="$$(cd app && swift build -c release --show-bin-path)/$(SWIFT_EXE)"; \
	if [ ! -x "$$swift_bin" ]; then \
		echo "ERROR: Swift executable not found at $$swift_bin" >&2; exit 1; \
	fi; \
	cp "$$swift_bin" "$(MACOS_DIR)/$(SWIFT_EXE)"
	@chmod +x "$(MACOS_DIR)/$(SWIFT_EXE)"
	@cp "$(BUILD_DIR)/ports" "$(RESOURCES_DIR)/ports"
	@chmod +x "$(RESOURCES_DIR)/ports"
	@cp "$(APP_ICON)" "$(RESOURCES_DIR)/AppIcon.icns"
	@printf '%s' '<?xml version="1.0" encoding="UTF-8"?>' > "$(INFO_PLIST)"
	@printf '%s' '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' >> "$(INFO_PLIST)"
	@{ \
		echo '<plist version="1.0">'; \
		echo '<dict>'; \
		echo '	<key>CFBundleIdentifier</key>'; \
		echo '	<string>$(BUNDLE_ID)</string>'; \
		echo '	<key>CFBundleName</key>'; \
		echo '	<string>$(APP_NAME)</string>'; \
		echo '	<key>CFBundleIconFile</key>'; \
		echo '	<string>AppIcon</string>'; \
		echo '	<key>CFBundleExecutable</key>'; \
		echo '	<string>$(SWIFT_EXE)</string>'; \
		echo '	<key>CFBundlePackageType</key>'; \
		echo '	<string>APPL</string>'; \
		echo '	<key>CFBundleShortVersionString</key>'; \
		echo '	<string>$(SHORT_VERSION)</string>'; \
		echo '	<key>CFBundleVersion</key>'; \
		echo '	<string>$(BUILD_VERSION)</string>'; \
		echo '	<key>LSMinimumSystemVersion</key>'; \
		echo '	<string>$(MIN_OS)</string>'; \
		echo '	<key>LSUIElement</key>'; \
		echo '	<true/>'; \
		echo '	<key>NSHighResolutionCapable</key>'; \
		echo '	<true/>'; \
		echo '</dict>'; \
		echo '</plist>'; \
	} >> "$(INFO_PLIST)"
	@plutil -lint "$(INFO_PLIST)"
	@echo "==> Bundle assembled."

# Ad-hoc codesign the assembled bundle.
sign: $(APP)
	@echo "==> Ad-hoc codesigning $(APP)"
	codesign --force --deep -s - "$(APP)"
	@echo "==> Verifying signature"
	codesign --verify --deep --strict "$(APP)"

clean:
	@echo "==> Removing $(BUILD_DIR)"
	@rm -rf $(BUILD_DIR)
