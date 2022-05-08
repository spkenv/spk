VERSION = $(shell cat spfs.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

.PHONY: rpm debug test
default: debug

debug:
	cd $(SOURCE_ROOT)
	cargo build --all

lint:
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
