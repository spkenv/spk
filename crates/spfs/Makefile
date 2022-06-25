VERSION = $(shell cat spfs.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

cargo_features_arg := $(if $(FEATURES),--features $(FEATURES))

.PHONY: rpm debug release test install install-debug setcap copy-release copy-debug
default: debug

debug:
	cd $(SOURCE_ROOT)
	cargo build --all $(cargo_features_arg)

release:
	cd $(SOURCE_ROOT)
	cargo build --all --release $(cargo_features_arg)

lint:
    # cargo fmt doesn't accept `--features`
	cargo fmt --check
	env RUSTDOCFLAGS="-Dwarnings" cargo doc -p spfs $(cargo_features_arg)
	cargo clippy --tests $(cargo_features_arg) -- -Dwarnings

test:
	cargo test --all $(cargo_features_arg)

rpm:
	cd $(SOURCE_ROOT)
	docker build . \
		--target build_env \
		--cache-from build_env \
		-f rpmbuild.Dockerfile \
		--tag build_env
	docker build . \
		--target spfs_build \
		--cache-from build_env \
		-f rpmbuild.Dockerfile \
		--build-arg VERSION=$(VERSION) \
		--tag spfs-rpm-builder
	mkdir -p .cache/cargo-registry
	mkdir -p dist/rpm
	docker run --rm \
		-v `pwd`/.cache/cargo-registry:/root/.cargo/registry \
		-v `pwd`/dist/rpm/RPMS:/root/rpmbuild/RPMS \
		spfs-rpm-builder

install-debug: copy-debug setcap

install: copy-release setcap

copy-debug: debug
	cd $(SOURCE_ROOT)
	sudo cp -f target/debug/spfs* /usr/bin/

copy-release: release
	cd $(SOURCE_ROOT)
	sudo cp -f target/release/spfs* /usr/bin/

setcap:
	sudo setcap 'cap_net_admin+ep' /usr/bin/spfs-monitor
	sudo setcap 'cap_chown,cap_fowner+ep' /usr/bin/spfs-render
	sudo setcap 'cap_sys_chroot,cap_sys_admin+e' /usr/bin/spfs-join
	sudo setcap 'cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep' /usr/bin/spfs-enter
