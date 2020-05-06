package spfs

import (
	"errors"
	"fmt"
	"os/exec"
	"strings"
)

var (
	ErrNoActiveRuntime = errors.New("No Active SPFS Runtime (run `spfs shell` first)")
	ErrUnknownTag      = errors.New("Unknown SpFS Tag")
)

func Reset(refs ...string) error {

	cmd := exec.Command("spfs", "reset", strings.Join(refs, "+"))
	return getError(cmd.CombinedOutput())

}

func ResetEditable(refs ...string) error {

	cmd := exec.Command("spfs", "reset", "--edit", strings.Join(refs, "+"))
	return getError(cmd.CombinedOutput())

}

func Edit() error {

	cmd := exec.Command("spfs", "edit")
	return getError(cmd.CombinedOutput())

}

func Run(refs []string, command string, args ...string) error {

	fullArgs := []string{
		"run", strings.Join(refs, "+"), "--", command,
	}
	fullArgs = append(fullArgs, args...)

	cmd := exec.Command("spfs", fullArgs...)
	return getError(cmd.CombinedOutput())

}

func CommitPlatform(tags ...string) error {

	return Commit("platform", tags...)

}

func CommitLayer(tags ...string) error {

	return Commit("platform", tags...)

}

func Commit(kind string, tags ...string) error {

	fullArgs := []string{
		"commit", kind,
	}
	for _, tag := range tags {
		fullArgs = append(fullArgs, "--tag", tag)
	}

	cmd := exec.Command("spfs", fullArgs...)
	return getError(cmd.CombinedOutput())

}

func getError(out []byte, err error) error {

	if err == nil {
		return nil
	}

	switch {

	case strings.Contains(string(out), "No active runtime"):
		return ErrNoActiveRuntime

	case strings.Contains(string(out), "Unknown tag"):
		return ErrUnknownTag

	default:
		return fmt.Errorf("%s[%w]", out, err)

	}

}
