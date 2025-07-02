SOURCE_ROOT := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

CARGO ?= cargo

spfs_packages = spfs,spfs-cli-main,spfs-cli-clean,spfs-cli-enter,spfs-cli-join,spfs-cli-render

comma := ,
cargo_features_arg = $(if $(FEATURES),--features $(FEATURES))
cargo_packages_arg := $(if $(CRATES),-p=$(CRATES))
cargo_packages_arg := $(subst $(comma), -p=,$(cargo_packages_arg))
cargo_packages_arg := $(if $(cargo_packages_arg),$(cargo_packages_arg),--workspace)

# Suppress this warning to not muddle the test output.
export SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING = 1

ifeq ($(OS),Windows_NT)
include Makefile.windows
else
include Makefile.linux
endif

# Create a file called "config.mak" to configure variables.
-include config.mak

default: build

.PHONY: act
act:
    # Run the github workflows locally using nektos/act.
    # `--privileged` needed for spfs privilege escalation
	act --privileged

.PHONY: packages
packages:
	$(MAKE) -C packages packages

.PHONY: packages.docker
packages.docker:
	$(MAKE) -C packages docker.packages

packages.%:
	$(MAKE) -C packages $*

.PHONY: clean
clean: packages.clean

.PHONY: lint
lint: FEATURES?=server,spfs/server
lint: lint-fmt lint-clippy lint-docs

.PHONY: lint-fmt
lint-fmt:
	$(CARGO) +nightly fmt --check

.PHONY: lint-clippy
lint-clippy:
	$(CARGO) clippy --tests $(cargo_features_arg) $(cargo_packages_arg) $(CARGO_ARGS) -- -Dwarnings

.PHONY: lint-docs
lint-docs:
	env RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps $(cargo_features_arg) $(cargo_packages_arg)

.PHONY: format
format:
	$(CARGO) +nightly fmt

.PHONY: build
build: debug

debug:
	$(CARGO) build $(cargo_packages_arg) $(cargo_features_arg) $(CARGO_ARGS)

debug-spfs:
	$(MAKE) debug CRATES=$(spfs_packages)

release:
	$(CARGO) build --release $(cargo_packages_arg) $(cargo_features_arg) $(CARGO_ARGS)

release-spfs:
	$(MAKE) release CRATES=$(spfs_packages)

.PHONY: test
test: FEATURES?=server,spfs/server
test:
	spfs run - -- cargo test $(cargo_features_arg) $(cargo_packages_arg) -- $(TEST_ARGS)

.PHONY: converters
converters:
	spk build spk-convert-pip

.PHONY: rpms
rpms: spk-rpm spfs-rpm

.PHONY: spfs-rpm
spfs-rpm:
	$(MAKE) rpm-build RPM_VERSION=$(SPFS_VERSION) RPM_APP=spfs

.PHONY: spk-rpm
spk-rpm:
	$(MAKE) rpm-build RPM_VERSION=$(SPK_VERSION) RPM_APP=spk

.PHONY: rpm-build
rpm-build: rpm-buildenv
	# ulimit for faster yum installs
	docker build . \
		--ulimit 'nofile=32768:32768' \
		--target rpm_build \
		--cache-from build_env \
		-f rpmbuild.Dockerfile \
		--build-arg VERSION=$(RPM_VERSION) \
		--build-arg APP=$(RPM_APP) \
		--tag spfs-rpm-builder
	mkdir -p .cache/cargo-registry
	mkdir -p dist/rpm
	docker run --rm --privileged \
		-v `pwd`/.cache/cargo-registry:/root/.cargo/registry:rw \
		-v `pwd`/dist/rpm/RPMS:/root/rpmbuild/RPMS:rw \
		spfs-rpm-builder

.PHONY: rpm-buildenv
rpm-buildenv:
	# ulimit for faster yum installs
	docker build . \
		--ulimit 'nofile=32768:32768' \
		--target build_env \
		--cache-from build_env \
		-f rpmbuild.Dockerfile \
		--tag build_env
