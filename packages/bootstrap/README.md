# Bootstrap Packages

Most of these packages somehow depend on themselves for the build process, and so require that some version already exist in order to create the packages. Usually you can use an older spk version of the package in question, but for entirely new spk environments these packages can be used to wrap system packages outside of spk and provide the necessary binaries to bootstrap the build process of creating actual spk packages.
