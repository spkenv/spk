package spm

import "fmt"

// Handle represents a defined package with attached
// payload(s) that can be read, downloaded, executed, etc.
type Handle interface {
	Spec() *Spec
	Url() string
}

type SpFSHandle struct {
	spec *Spec
	ref  string
}

func NewSpFSHandle(spec *Spec, ref string) *SpFSHandle {
	return &SpFSHandle{spec, ref}
}

func (h SpFSHandle) Spec() *Spec {
	return h.spec
}

func (h SpFSHandle) Url() string {
	return "spfs:/" + h.ref
}

func (h SpFSHandle) String() string {
	return fmt.Sprintf("%s | %s", h.spec.Package, h.Url())
}
