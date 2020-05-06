package spm

import (
	"bytes"
	"fmt"
	"testing"

	"gopkg.in/yaml.v2"
)

func ExampleParseSpec() {

	spec, err := ParseSpec(`
pkg: hello_world/1.0.0
depends:
- pkg: output/0.1.9`)
	if err != nil {
		panic(err)
	}
	fmt.Println(spec.Package)

	// Output:
	// hello_world/1.0.0

}

func TestIdentMarshaling(t *testing.T) {

	spec := Ident{
		Name: "package",
	}
	yamlString, err := yaml.Marshal(spec)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Equal(yamlString, []byte("")) {
		t.Fatal("yaml string should not be empty")
	}

}

func TestIdentUnmarshaling(t *testing.T) {

	spec := Ident{}
	err := yaml.Unmarshal([]byte("package/1.0.2/r2"), &spec)
	if err != nil {
		t.Fatal(err)
	}
	if spec.Name != "package" {
		t.Errorf("expected package name to be separated out: (%s != %s)", spec.Name, "package")
	}
	if spec.Version.String() != "1.0.2" {
		t.Errorf("expected package version to be separated out: (%s != %s)", spec.Version, "1.0.2")
	}
	if spec.Release.String() != "r2" {
		t.Errorf("expected package release to be separated out: (%s != %s)", spec.Release, "r2")
	}

}

func TestIdentUnmarshalingNonString(t *testing.T) {

	spec := Ident{}
	err := yaml.Unmarshal([]byte("{}"), &spec)
	if err == nil {
		t.Fatal("expected error unmarshalling non-string to package spec")
	}

}
