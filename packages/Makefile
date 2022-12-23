export SPFS_FILESYSTEM_TMPFS_SIZE=10G
INTERACTIVE:=$(shell [ -t 0 ] && echo 1)
ifdef INTERACTIVE
# docker and the commands run under docker will be easier to
# manage and have nicer output if we allocate a tty, but this only
# works when the calling process is actually an interactive shell
DOCKER_EXTRA_ARGS=-t
endif

.PHONY: packages
packages: \
	bootstrap \
	gnu \
	perl \
	python \
	pip/pip.spk \
	cmake \
	ninja \
	opentimelineio

.PHONY: bootstrap boostrap.full
# boostrap just enough to get a real linker and compiler
# under /spfs, then move on from there
bootstrap: \
	stdfs/stdfs.spk \
	bootstrap/automake.spk \
	bootstrap/gcc.spk \
	bootstrap/glibc.spk
bootstrap.full: bootstrap \
	bootstrap/autoconf.spk \
	bootstrap/autoconf-archive.spk \
	bootstrap/autogen.spk \
	bootstrap/bison.spk \
	bootstrap/coreutils.spk \
	bootstrap/flex.spk \
	bootstrap/grep.spk \
	bootstrap/libtool.spk \
	bootstrap/m4.spk \
	bootstrap/make.spk \
	bootstrap/perl.spk \
	bootstrap/sed.spk \
	bootstrap/texinfo.spk \
	bootstrap/help2man.spk \
	bootstrap/gettext.spk \
	bootstrap/zip.spk \
	bootstrap/zlib.spk

.PHONY: gnu
gnu: bootstrap.full \
	gnu/binutils.spk \
	gnu/bash.spk \
	gnu/ncurses.spk \
	gnu/gmp.spk \
	gnu/mpfr.spk \
	gnu/mpc.spk \
	gnu/gcc/gcc48.spk \
	gnu/gcc/gcc63.spk
gnu.full: gnu \
	gnu/autoconf.spk \
	gnu/automake.spk \
	gnu/autoconf-archive.spk \
	gnu/make.spk \
	gnu/glibc.spk \
	gnu/m4.spk \
	gnu/texinfo.spk \
	gnu/gcc/gcc93.spk

perl: gnu perl/perl.spk

.PHONY: python python2 python3
python: python2 python3
python2: bootstrap bzip2/bzip2.spk zlib/zlib.spk python/python2.spk
python3: bootstrap bzip2/bzip2.spk zlib/zlib.spk libffi/libffi.spk openssl/openssl.spk python/python3.spk

.PHONY: ninja
ninja: ninja/ninja.spk

zlib/zlib.spk: cmake

.PHONY: cmake
cmake: cmake/cmake.spk


.PHONY: opentimelineio
opentimelineio: python3 opentimelineio/opentimelineio.spk

.PHONY: clean
clean:
	find . -name "*.spk" -delete

.PHONY: import
import:
	find . -name "*.spk" -not -path "./bootstrap/*" | xargs spk import

%: $</$<.spk

%.spk : %.spk.yaml
	spk info -r local $<@source > /dev/null 2>&1 || spk make-source -v $<
	spk mkb -r origin -v $<
	spk export $< $@

# a specific package.spk.yaml.in takes precedence over the
# generic rule below that uses the simple spk.yaml.in
bootstrap/%.spk.yaml : bootstrap/%.spk.yaml.in
	export VERSION=$$(rpm -q --queryformat '%{VERSION}' $*) && \
	if [ ! -f $@ ]; then cp $< $@; fi && \
	sed -i "s|VERSION|$${VERSION}|g" $@ && \
	touch $@

# the above rule must appear first to take precedence over this one
bootstrap/%.spk.yaml : bootstrap/spk.yaml.in
	export VERSION=$$(rpm -q --queryformat '%{VERSION}' $*) && \
	if [ ! -f $@ ]; then cp $< $@; fi && \
	sed -i 's|NAME|$*|g' $@ && \
	sed -i "s|VERSION|$${VERSION}|g" $@ && \
	touch $@

.PHONY: lint
lint: $(shell find . -name "*.spk.yaml")
	spk lint $^

docker.%:
	if [ ! -f ../dist/rpm/RPMS/x86_64/spk-*.rpm ]; then \
	echo "Please run 'make rpm' or download the latest spk rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	cp builder.spfs.toml ../dist
	docker build ../dist -t spk-package-builder -f Dockerfile
	-e SPFS_FILESYSTEM_TMPFS_SIZE \
	-e RUST_LOG \
	-v $$PWD/../build/packages/:/spfs-storage \
	-v $$PWD/..:/work spk-package-builder bash -ex -c "\
	cd /work && \
	make packages.$*"
