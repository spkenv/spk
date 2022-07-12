VERSION = $(shell grep Version spk.spec | cut -d ' ' -f 2)

# Create a file called "config.mak" to configure variables.
-include config.mak

default: lint test

.PHONY: packages
packages:
	$(MAKE) -C packages packages

packages.docker:
	$(MAKE) -C packages docker.packages

packages.%:
	$(MAKE) -C packages $*

.PHONY: clean
clean: packages.clean

.PHONY: lint lint-rust
lint: lint-rust
lint-rust:
	cargo fmt --check
	cargo clippy --tests -- -Dwarnings
	# also check SPI's configuration
	cargo clippy --tests --features "sentry, migration-to-components" -- -Dwarnings
	env RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps

.PHONY: format
format:
	cargo fmt

.PHONY: test test-rust
test: test-rust
test-rust:
	spfs run - -- cargo test

converters:
	$(MAKE) -C packages spk-convert-pip/spk-convert-pip.spk

.PHONY: rpm
rpm: SPFS_PULL_USERNAME ?= $(shell read -p "Github Username: " user; echo $$user)
rpm: SPFS_PULL_PASSWORD ?= $(shell read -s -p "Github Password/Access Token: " pass; echo $$pass)
rpm:
	cd $(SOURCE_ROOT)
	docker build . \
		-f rpmbuild.Dockerfile \
		--build-arg VERSION=$(VERSION) \
		--build-arg SPFS_PULL_USERNAME=$(SPFS_PULL_USERNAME) \
		--build-arg SPFS_PULL_PASSWORD=$(SPFS_PULL_PASSWORD) \
		--tag spk-rpm-builder
	mkdir -p dist/rpm
	CONTAINER=$$(docker create spk-rpm-builder) \
	  && docker cp $$CONTAINER:/root/rpmbuild/RPMS dist/rpm/ \
	  && docker rm --force $$CONTAINER
