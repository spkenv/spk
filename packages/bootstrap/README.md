# Bootstrap Packages

Some  packages somehow depend on themselves (like gcc) for the build process, and so require that some version already exist in order to create the packages. Usually you can use an older spk version of the package in question, but for entirely new spk environments these packages can be used to wrap system packages outside of spk and provide the necessary binaries to bootstrap the build process of creating actual spk packages.

The `packages/Makefile` includes a rule to generate a number of these based on binaries installed in the current machine. These packages are created with version `0.0.0+bootstrap.0`. They are not intended to be published, and will taint any package that uses them with the variable `var: boostrap/yes`.
