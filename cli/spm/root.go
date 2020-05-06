package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var root = cobra.Command{
	Use:   "spm",
	Short: "The 'S' Package Manger",
	Long:  "Convenience, clarity and speed.",
}

func main() {
	err := root.Execute()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
