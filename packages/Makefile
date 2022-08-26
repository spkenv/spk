.PHONY: packages
packages: \
	bootstrap \
	gnu \
	perl \
	python \
	pip/pip.spk

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

zlib/zlib.spk: cmake/cmake.spk

.PHONY: clean
clean:
	find . -name "*.spk" -delete

.PHONY: import
import:
	find . -name "bootstrap" -prune -o -name "*.spk" -exec spk import {} \;

%: $</$<.spk

%.spk : %.spk.yaml
	spk info -r local $<@source > /dev/null 2>&1 || spk make-source -v $<
	spk mkb -r origin -v $<
	spk export $< $@

.PRECIOUS: bootstrap/*.spk.yaml
bootstrap/%.spk.yaml : bootstrap/spk.yaml.in
	if [ ! -f $@ ]; then \
	cp $< $@ && \
	sed -i 's|NAME|$*|g' $@; \
	else touch $@; fi

docker.%:
	if [ ! -f ../dist/rpm/RPMS/x86_64/spk-*.rpm ]; then \
	echo "Please run 'make rpm' or download the latest spk rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	cp builder.spfs.toml ../dist
	docker build ../dist -t spk-package-builder -f Dockerfile
	docker run --privileged --rm -i \
	-e SPFS_FILESYSTEM_TMPFS_SIZE=10G \
	-e RUST_LOG \
	-v $$PWD/../build/packages/:/spfs-storage \
	-v $$PWD/..:/work spk-package-builder bash -ex -c "\
	cd /work && \
	make packages.$*"
