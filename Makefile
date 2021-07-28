VERSION = $(shell cat spk.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

.PHONY: rpm devel test packages clean lint
default: devel

packages:
	cd $(SOURCE_ROOT)/packages && \
	$(MAKE) packages

packages.%:
	cd $(SOURCE_ROOT)/packages && $(MAKE) $*

clean: packages.clean

lint:
	pipenv run -- mypy spk
	pipenv run -- black --check spk setup.py

devel:
	cd $(SOURCE_ROOT)
	pipenv run -- python setup.py develop

test:
	cd $(SOURCE_ROOT)
	mkdir -p /tmp/spfs-runtimes
	SPFS_STORAGE_RUNTIMES="/tmp/spfs-runtimes" \
	pipenv run -- spfs run - -- pytest -x -vvv

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
