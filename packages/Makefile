#!/usr/bin/bash

SPECS := $(wildcard packages/**/*.spk.yaml)

.PHONY: packages
packages: \
	bootstrap \
	packages/stdfs/stdfs.mark \
	gnu \
	python \
	packages/pip/pip.mark

.PHONY: bootstrap
boostrap: \
	packages/bootstrap/gcc.mark \
	packages/boostrap/make.mark

.PHONY: gnu
gnu: \
	packages/gnu/make.mark \
	packages/gnu/autoconf.mark \
	packages/gnu/gmp.mark \
	packages/gnu/mpfr.mark \
	packages/gnu/mpc.mark \
	packages/gnu/gcc/gcc48.mark \
	packages/gnu/gcc/gcc63.mark \
	packages/gnu/gcc/gcc93.mark

.PHONY: python
python: \
	packages/python/python2.mark \
	packages/python/python3.mark

.PHONY: clean
clean:
	find packages -name "*.mark" -delete

%.mark :: %.spk.yaml
	spk info $<@source > /dev/null 2>&1 || spk make-source $<
	spk mkb -dr origin -l $<
	touch $@
