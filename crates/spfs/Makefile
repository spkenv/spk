VERSION = $(shell cat spfs.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

cargo_features_arg := $(if $(FEATURES),--features $(FEATURES))

.PHONY: rpm debug release test install
default: debug

debug:
	cd $(SOURCE_ROOT)
	cargo build --all $(cargo_features_arg)

release:
	cd $(SOURCE_ROOT)
	cargo build --all --release $(cargo_features_arg)

lint:
	cargo fmt --check
	cargo clippy --tests -- -Dwarnings

test:
	cargo test --all

rpm:
	cd $(SOURCE_ROOT)
	docker build . \
		-f rpmbuild.Dockerfile \
		--build-arg VERSION=$(VERSION) \
		--tag spfs-rpm-builder
	mkdir -p dist/rpm
	CONTAINER=$$(docker create spfs-rpm-builder) \
	  && docker cp $$CONTAINER:/root/rpmbuild/RPMS dist/rpm/ \
	  && docker rm --force $$CONTAINER

install: release
	cd $(SOURCE_ROOT)
	sudo cp target/release/spfs* /usr/bin/
	sudo setcap 'cap_chown,cap_fowner+ep' /usr/bin/spfs-render
	sudo setcap 'cap_sys_chroot,cap_sys_admin+e' /usr/bin/spfs-join
	sudo setcap 'cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep' /usr/bin/spfs-enter
