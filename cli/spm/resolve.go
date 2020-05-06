package main

import (
	"fmt"

	"github.com/spf13/cobra"

	"gitlab.spimageworks.com/dev-group/dev-ops/spm"
)

var resolveCmd = &cobra.Command{
	Use:   "resolve PKG [PKG...]",
	Short: "Resolve a set of packages",
	Long:  "View and introspect the package resolution process",
	RunE:  runResolve,
}

func init() {
	root.AddCommand(resolveCmd)
}

func runResolve(cmd *cobra.Command, args []string) error {

	var (
		err   error
		specs = make([]spm.Ident, len(args))
	)
	for i := 0; i < len(args); i++ {
		specs[i], err = spm.ParseIdent(args[i])
		if err != nil {
			return err
		}
	}
	for _, spec := range specs {
		fmt.Println("requested: ", spec)
	}
	return nil
}
