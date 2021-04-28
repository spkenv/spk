#!/usr/bin/bash

SPECS := $(wildcard **/*.spk.yaml)

.PHONY: packages
packages: \
	bootstrap \
	gnu \
	perl \
	python \
	pip/pip.mark

.PHONY: bootstrap
# boostrap just enough to get a real linker and compiler
# under /spfs, then move on from there
bootstrap: \
	stdfs/stdfs.mark \
	bootstrap/autoconf.mark \
	bootstrap/autogen.mark \
	bootstrap/automake.mark \
	bootstrap/bison.mark \
	bootstrap/coreutils.mark \
	bootstrap/flex.mark \
	bootstrap/gcc.mark \
	bootstrap/grep.mark \
	bootstrap/libtool.mark \
	bootstrap/m4.mark \
	bootstrap/make.mark \
	bootstrap/perl.mark \
	bootstrap/sed.mark \
	bootstrap/texinfo.mark \
	bootstrap/zip.mark

.PHONY: gnu
gnu: \
	gnu/binutils.mark \
	gnu/bash.mark \
	gnu/autoconf-archive.mark \
	gnu/gmp.mark \
	gnu/mpfr.mark \
	gnu/mpc.mark \
	gnu/gcc/gcc48.mark \
	gnu/gcc/gcc63.mark \
	gnu/gcc/gcc93.mark \
	gnu/make.mark \
	gnu/m4.mark \
	gnu/texinfo.mark

.PHONY: perl
perl: \
	perl/perl.mark

.PHONY: python
python: \
	python/python2.mark \
	python/python3.mark

.PHONY: clean
clean:
	find packages -name "*.mark" -delete

%.mark : %.spk.yaml
	spk info -l $<@source > /dev/null 2>&1 || spk make-source $<
	spk mkb -dr origin -l $<
	touch $@

.PRECIOUS: bootstrap/*.spk.yaml
bootstrap/%.spk.yaml : bootstrap/spk.yaml.in
	if [ ! -f $@ ]; then \
	cp $< $@ && \
	sed -i 's|NAME|$*|g' $@; \
	else touch $@; fi
