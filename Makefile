SPK_VERSION = $(shell grep Version spk.spec | cut -d ' ' -f 2)
SPFS_VERSION = $(shell cat spfs.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))
CARGO_TARGET_DIR := $(shell \
	if test -f .cargo/config.toml; \
	then (grep target-dir .cargo/config.toml || echo target) | sed -sE 's|.*"(.*)".*|\1|'; \
	else echo target; \
	fi)
CARGO ?= cargo

spfs_packages = spfs,spfs-cli-main,spfs-cli-clean,spfs-cli-enter,spfs-cli-join,spfs-cli-render

export PLATFORM ?= unix
ifeq ($(PLATFORM),windows)
CARGO_ARGS += --target x86_64-pc-windows-gnu
# swap cargo for cross when building for other platforms
CARGO = cross
else
spfs_packages := $(spfs_packages),spfs-cli-fuse,spfs-cli-monitor
endif

comma := ,
cargo_features_arg = $(if $(FEATURES),--features $(FEATURES))
cargo_packages_arg := $(if $(CRATES),-p=$(CRATES))
cargo_packages_arg := $(subst $(comma), -p=,$(cargo_packages_arg))
cargo_packages_arg := $(if $(cargo_packages_arg),$(cargo_packages_arg),--workspace)

# Suppress this warning to not muddle the test output.
export SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING = 1

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
lint:
	$(CARGO) +nightly fmt --check
	$(CARGO) clippy --tests $(cargo_features_arg) $(cargo_packages_arg) -- -Dwarnings
	env RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps $(cargo_features_arg) $(cargo_packages_arg)

.PHONY: format
format:
	$(CARGO) +nightly fmt

.PHONY: build
build: debug

debug:
	cd $(SOURCE_ROOT)
	$(CARGO) build $(cargo_packages_arg) $(cargo_features_arg) $(CARGO_ARGS)

debug-spfs:
	$(MAKE) debug CRATES=$(spfs_packages)

release:
	cd $(SOURCE_ROOT)
	$(CARGO) build --release $(cargo_packages_arg) $(cargo_features_arg) $(CARGO_ARGS)

release-spfs:
	$(MAKE) release CRATES=$(spfs_packages)

.PHONY: test
test: FEATURES?=server,spfs/server
test:
	spfs run - -- cargo test $(cargo_features_arg) $(cargo_packages_arg)

.PHONY: converters
converters:
	$(MAKE) -C packages spk-convert-pip/spk-convert-pip.spk

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
	docker build . \
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
	cd $(SOURCE_ROOT)
	docker build . \
		--target build_env \
		--cache-from build_env \
		-f rpmbuild.Dockerfile \
		--tag build_env

install-debug-spfs: copy-debug-spfs setcap

install-debug-spk: copy-debug-spk

install-debug: install-debug-spfs install-debug-spk

install: copy-release setcap

install-spfs: copy-spfs setcap

copy-debug-spfs: debug-spfs
	cd $(SOURCE_ROOT)
	sudo cp -f $(CARGO_TARGET_DIR)/debug/spfs* /usr/local/bin/

copy-debug-spk: debug
	cd $(SOURCE_ROOT)
	sudo cp -f $(CARGO_TARGET_DIR)/debug/spk /usr/local/bin/

copy-debug: copy-debug-spfs copy-debug-spk

copy-release: release
	cd $(SOURCE_ROOT)
	sudo cp -f $(CARGO_TARGET_DIR)/release/spk $(CARGO_TARGET_DIR)/release/spfs* /usr/local/bin/

copy-spfs: release-spfs
	cd $(SOURCE_ROOT)
	sudo cp -f $(CARGO_TARGET_DIR)/release/spfs* /usr/local/bin/

setcap:
	sudo setcap 'cap_dac_override,cap_fowner+ep' /usr/local/bin/spfs-clean
	sudo setcap 'cap_net_admin+ep' /usr/local/bin/spfs-monitor
	sudo setcap 'cap_chown,cap_fowner+ep' /usr/local/bin/spfs-render
	sudo setcap 'cap_sys_chroot,cap_sys_admin+ep' /usr/local/bin/spfs-join
	sudo setcap 'cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep' /usr/local/bin/spfs-enter
	sudo setcap 'cap_sys_admin+ep' /usr/local/bin/spfs-fuse
