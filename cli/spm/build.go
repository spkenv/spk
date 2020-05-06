package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"gitlab.spimageworks.com/dev-group/dev-ops/spm"
)

var buildCmd = &cobra.Command{
	Use:   "build SPEC_FILE",
	Short: "Build a package",
	Long:  "Build a package from it's spec file",
	Args:  cobra.ExactArgs(1),
	Run:   runBuild,
}

func init() {
	root.AddCommand(buildCmd)
}

func runBuild(cmd *cobra.Command, args []string) {

	filename := args[0]
	spec, err := spm.ReadSpec(filename)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	handles, err := spm.BuildVariants(spec)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	for _, handle := range handles {
		fmt.Printf("Created: %s\n", handle)
	}

}
