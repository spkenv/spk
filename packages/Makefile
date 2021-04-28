#!/usr/bin/bash

SPECS := $(wildcard **/*.spk.yaml)

.PHONY: packages
packages: \
	bootstrap \
	gnu \
	perl \
	python \
	pip/pip.spk

.PHONY: bootstrap
# boostrap just enough to get a real linker and compiler
# under /spfs, then move on from there
bootstrap: \
	stdfs/stdfs.spk \
	bootstrap/autoconf.spk \
	bootstrap/autogen.spk \
	bootstrap/automake.spk \
	bootstrap/bison.spk \
	bootstrap/coreutils.spk \
	bootstrap/flex.spk \
	bootstrap/gcc.spk \
	bootstrap/grep.spk \
	bootstrap/libtool.spk \
	bootstrap/m4.spk \
	bootstrap/make.spk \
	bootstrap/perl.spk \
	bootstrap/sed.spk \
	bootstrap/texinfo.spk \
	bootstrap/zip.spk

.PHONY: gnu
gnu: \
	gnu/binutils.spk \
	gnu/bash.spk \
	gnu/autoconf-archive.spk \
	gnu/gmp.spk \
	gnu/mpfr.spk \
	gnu/mpc.spk \
	gnu/gcc/gcc48.spk \
	gnu/gcc/gcc63.spk \
	gnu/gcc/gcc93.spk \
	gnu/make.spk \
	gnu/m4.spk \
	gnu/texinfo.spk

.PHONY: perl
perl: \
	perl/perl.spk

.PHONY: python
python: \
	python/python2.spk \
	python/python3.spk

.PHONY: clean
clean:
	find . -name "*.spk" -delete

.PHONY: import
import:
	find . -name "*.spk" -exec spk import {} \;

%.spk : %.spk.yaml
	spk info -l $<@source > /dev/null 2>&1 || spk make-source $<
	spk mkb -dr origin -l $<
	spk export $< $@

.PRECIOUS: bootstrap/*.spk.yaml
bootstrap/%.spk.yaml : bootstrap/spk.yaml.in
	if [ ! -f $@ ]; then \
	cp $< $@ && \
	sed -i 's|NAME|$*|g' $@; \
	else touch $@; fi
