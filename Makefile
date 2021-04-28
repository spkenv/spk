VERSION = $(shell cat spk.spec | grep Version | cut -d ' ' -f 2)
SOURCE_ROOT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))

.PHONY: rpm devel test packages clean
default: devel

packages:
	cd $(SOURCE_ROOT)/packages && \
	$(MAKE) packages


packages-docker:
	if [ ! -f dist/rpm/RPMS/x86_64/spk-*.rpm ]; then \
	echo "Please run 'make rpm' or download the latest spk rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	if [ ! -f dist/rpm/RPMS/x86_64/spfs-*.rpm ]; then \
	echo "Please build the spfs rpm or download the latest spfs rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	docker run --privileged --rm \
	-v $(SOURCE_ROOT)/dist/rpm/RPMS/x86_64:/rpms \
	-v $(SOURCE_ROOT):/work centos:7 bash -c "\
	yum install -y /rpms/* gcc gcc-c++ autoconf autogen automake bison coreutils flex grep libtool m4 make perl sed texinfo zip && \
	mkdir -p origin/{objects,payloads,tags} && \
	cd /work && \
	make packages"

packages-import:
	cd $(SOURCE_ROOT)/packages && make import

clean:
	cd $(SOURCE_ROOT)/packages && make clean

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
