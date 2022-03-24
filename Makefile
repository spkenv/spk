VERSION = $(shell grep Version spk.spec | cut -d ' ' -f 2)

# Create a file called "config.mak" to configure variables.
-include config.mak

default: devel

.PHONY: packages
packages:
	$(MAKE) -C packages packages

packages.docker:
	$(MAKE) -C packages docker.packages

packages.%:
	$(MAKE) -C packages $*

.PHONY: clean
clean: packages.clean

.PHONY: lint lint-python lint-rust
lint: lint-rust lint-python
lint-rust:
	# we need to ingore this clippy warning until the next
	# release of py03 which solves for it
	cargo clippy -- -Dwarnings -Aclippy::needless_option_as_deref
lint-python:
	pipenv run -- mypy spk spkrs
	pipenv run -- black --check spk setup.py spkrs packages/spk-convert-pip/spk-convert-pip

.PHONY: format
format:
	pipenv run -- black spk setup.py spkrs packages/spk-convert-pip/spk-convert-pip

.PHONY: devel
devel:
	pipenv run -- python setup.py develop

.PHONY: test test-python test-rust
test: test-rust test-python
test-rust:
	# other tooling (rust-analyzer) can create
	# unhappy builds of pyo3 which cause the build of
	# the tests to fail
	cargo clean -p pyo3 && cargo test --no-default-features
test-python:
	mkdir -p /tmp/spfs-runtimes
	SPFS_STORAGE_RUNTIMES="/tmp/spfs-runtimes" \
	pipenv run -- spfs run - -- pytest -x -vvv

.PHONY: cargo-test
cargo-test:
	# some tests must be run in an spfs environment
	spfs run - -- cargo test --no-default-features

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
