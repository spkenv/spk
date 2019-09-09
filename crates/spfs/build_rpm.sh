#!/usr/bin/bash

rpmdev-setuptree
rpmbuild -ba spenv.spec
