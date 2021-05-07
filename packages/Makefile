# disables exporting packages in favour of touching the file, saves time and space
NOEXPORT ?= 0

.PHONY: packages
packages: \
	bootstrap \
	gnu \
	perl \
	python

.PHONY: bootstrap bootstrap.phase1 bootstrap.phase2
bootstrap: bootstrap.phase1 bootstrap.phase2
bootstrap.phase1: \
	bootstrap/binutils.spk \
	bootstrap/gcc.spk \
	stdfs/stdfs.spk \
	bootstrap/glibc.spk \
	bootstrap/libstdcxx.spk
bootstrap.phase2: \
	bootstrap/m4.spk \
	bootstrap/ncurses.spk \
	bootstrap/bash.spk \
	bootstrap/coreutils.spk \
	bootstrap/diffutils.spk \
	bootstrap/file.spk \
	bootstrap/findutils.spk \
	bootstrap/gawk.spk \
	bootstrap/grep.spk \
	bootstrap/gzip.spk \
	bootstrap/make.spk \
	bootstrap/patch.spk \
	bootstrap/sed.spk \
	bootstrap/tar.spk \
	bootstrap/xz.spk \
	bootstrap/binutils2.spk \
	bootstrap/gcc2.spk \
	bootstrap/libstdcxx2.spk

.PHONY: gnu

.PHONY: gnu
gnu: \
	gettext.spk \
	bison.spk \
	perl.spk \
	texinfo.spk \
	util-linux.spk

#####################################

perl: perl/perl.spk

.PHONY: python python2 python3
python: python2 python3
python2: bootstrap bzip2 python/python2.spk
python3: bootstrap bzip2 python/python3.spk

.PHONY: clean
clean:
	find . -name "*.spk" -delete

.PHONY: import
import:
	find . -name "bootstrap" -prune -o -name "*.spk" -exec spk import {} \;

# bzip2: bzip2/bzip2.spk
%: $</$<.spk

%.spk : %.spk.yaml
	spk info -l $<@source > /dev/null 2>&1 || spk make-source $<
	SPFS_FILESYSTEM_TMPFS_SIZE=30G \
	spk mkb -dr origin -l -vv $<
	if [ "$(NOEXPORT)" == "1" ]; then touch $@; else spk export $< $@; fi

docker.%:
	if [ ! -f ../dist/rpm/RPMS/x86_64/spk-*.rpm ]; then \
	echo "Please run 'make rpm' or download the latest spk rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	if [ ! -f ../dist/rpm/RPMS/x86_64/spfs-*.rpm ]; then \
	echo "Please build the spfs rpm or download the latest spfs rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	docker build . -t spk-package-builder
	docker run --privileged --rm \
	-v $$PWD/../build/packages/:/spfs-storage \
	-v $$PWD/../dist/rpm/RPMS/x86_64:/rpms \
	-v $$PWD/..:/work spk-package-builder bash -ex -c "\
	yum install -y /rpms/* && \
	cd /work && \
	make packages.$*"
