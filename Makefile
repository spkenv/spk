SPK_VERSION = $(shell grep Version spk.spec | cut -d ' ' -f 2)
SPFS_VERSION = $(shell cat spfs.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

cargo_features_arg = $(if $(FEATURES),--features $(FEATURES))

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
lint:
	cargo fmt --check
	cargo clippy --tests $(cargo_features_arg) -- -Dwarnings
	env RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps $(cargo_features_arg)

.PHONY: format
format:
	cargo fmt

.PHONY: build
build: debug

debug: FEATURES ?= spfs/cli
debug:
	cd $(SOURCE_ROOT)
	cargo build --workspace $(cargo_features_arg)

debug-spfs: FEATURES ?= spfs/cli
debug-spfs:
	cd $(SOURCE_ROOT)
	cargo build -p spfs $(cargo_features_arg)

release: FEATURES ?= spfs/cli
release:
	cd $(SOURCE_ROOT)
	cargo build --workspace --release $(cargo_features_arg)

.PHONY: test
test:
	spfs run - -- cargo test --workspace $(cargo_features_arg)

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

copy-debug-spfs: debug-spfs
	cd $(SOURCE_ROOT)
	sudo cp -f target/debug/spfs* /usr/bin/

copy-debug-spk: debug
	cd $(SOURCE_ROOT)
	sudo cp -f target/debug/spk /usr/bin/

copy-debug: copy-debug-spfs copy-debug-spk

copy-release: release
	cd $(SOURCE_ROOT)
	sudo cp -f target/release/spk target/release/spfs* /usr/bin/

setcap:
	sudo setcap 'cap_net_admin+ep' /usr/bin/spfs-monitor
	sudo setcap 'cap_chown,cap_fowner+ep' /usr/bin/spfs-render
	sudo setcap 'cap_sys_chroot,cap_sys_admin+ep' /usr/bin/spfs-join
	sudo setcap 'cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep' /usr/bin/spfs-enter
