#!/usr/bin/bash

rpmdev-setuptree
rpmbuild -ba spenv.spec --define "_rpmdir $(pwd)/build/rpm"
