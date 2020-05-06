package spm

import (
	"fmt"
	"os"
	"os/exec"

	"gitlab.spimageworks.com/dev-group/dev-ops/spm/internal/spfs"
)

const (
	defaultBuildCommand = "bash build.sh"
)

// BuildVariants builds all of the default variants defined for the given spec
func BuildVariants(spec *Spec) ([]Handle, error) {

	variants := spec.Build.Variants
	if len(variants) == 0 {
		variants = []OptionMap{OptionMap{}}
	}

	handles := make([]Handle, len(variants))
	for i, options := range variants {
		h, err := Build(spec, options)
		if err != nil {
			return nil, fmt.Errorf("Failed to build variant %d [%s]: %w", i, options.Digest(), err)
		}
		handles[i] = h
	}
	return handles, nil
}

// Build executes the build process for a package spec with the given build options
func Build(spec *Spec, options OptionMap) (Handle, error) {

	cmdString := spec.Build.Command
	if cmdString == "" {
		cmdString = defaultBuildCommand
	}

	release := options.Digest()
	fmt.Printf("|--| building: %s/%s |--|\n", spec.Package.String(), release)
	for _, opt := range spec.Options {
		value, given := options[opt.Package.Name]
		if !given {
			// TODO: get from environment? get default?
		}
		fmt.Printf("%s: %s\n", opt.Package.Name, value)
	}

	// TODO: get build dependencies
	deps := make([]string, len(spec.Options))
	for i, dep := range spec.Options {
		// TODO: what if the dep.Package already has a version/release?
		tag := fmt.Sprintf("spm/pkg/%s/%s", dep.Package.Name, options[dep.Package.Name])
		deps[i] = tag
	}

	err := spfs.ResetEditable(deps...)
	if err != nil {
		return nil, err
	}

	cmd := exec.Command("sh", "-c", cmdString)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	fmt.Printf("|--| %s |--| \n", cmd)
	err = cmd.Run()
	if err != nil {
		return nil, fmt.Errorf("build command failed: %w", err)
	}

	// TODO: check that there are file changes
	// TODO: check that there are no overwritten files

	tag := "spm/pkg/" + spec.Package.String()
	err = spfs.CommitLayer(tag)
	if err != nil {
		return nil, fmt.Errorf("failed to commit package data to spfs: %w", err)
	}

	return NewSpFSHandle(spec, tag), nil
}
