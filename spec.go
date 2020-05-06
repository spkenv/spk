package spm

import (
	"bytes"
	"crypto/sha1"
	"encoding/base32"
	"fmt"
	"io/ioutil"
	"sort"
	"strings"

	"gopkg.in/yaml.v2"
)

const (
	// given option digests are namespaced by the package itself,
	// there are slim likelyhoods of collision, so we roll the dice
	digestSize = 7
)

// Spec encompases the complete specification of a package
type Spec struct {
	Package  Ident     `yaml:"pkg"`
	Build    BuildSpec `yaml:"build"`
	Options  []Spec    `yaml:"opts"`
	Depends  []Spec    `yaml:"depends"`
	Provides []Spec    `yaml:"provides"`
}

// ReadSpec loads a package specification from a yaml file
func ReadSpec(filepath string) (*Spec, error) {

	bytes, err := ioutil.ReadFile(filepath)
	if err != nil {
		return nil, fmt.Errorf("Failed to read specification file: %w", err)
	}
	return ParseSpec(string(bytes))
}

// ParseSpec parses the raw yaml string of
// a package specification
func ParseSpec(source string) (*Spec, error) {

	def := new(Spec)
	dec := yaml.NewDecoder(bytes.NewReader([]byte(source)))
	dec.SetStrict(true)
	err := dec.Decode(&def)
	if err != nil {
		return nil, fmt.Errorf("Failed to read specification: %w", err)
	}
	return def, nil

}

// Ident represents a package identifier
//
// The identifier is either a specific package or
// range of package versions/releases depending on the
// syntax and context
type Ident struct {
	Name    string
	Version Version
	Release Release
}

// ParseIdent parses a package identifier string
func ParseIdent(source string) (Ident, error) {
	spec := new(Ident)
	return *spec, spec.ParseIdent(source)
}

// ParseIdent parses an identifier string into this structure
func (spec *Ident) ParseIdent(source string) error {
	parts := strings.Split(source, "/")

	var name, version, release string
	switch len(parts) {
	case 1:
		name = parts[0]
	case 2:
		name, version = parts[0], parts[1]
	case 3:
		name, version, release = parts[0], parts[1], parts[2]
	default:
		return fmt.Errorf("too many components: %s", source)
	}

	spec.Name = name
	spec.Version = ParseVersion(version)
	spec.Release = ParseRelease(release)
	return nil
}

func (spec Ident) String() string {
	specString := fmt.Sprintf("%s/%s", spec.Name, spec.Version)
	if spec.Release.String() != "" {
		specString += "/" + spec.Release.String()
	}
	return specString
}

// MarshalYAML turns this package spec into a yaml string
func (spec Ident) MarshalYAML() (interface{}, error) {
	return spec.String(), nil
}

// UnmarshalYAML allows this spec to be parsed directly in a yaml structure
func (spec *Ident) UnmarshalYAML(unmarshal func(interface{}) error) error {

	var source string
	err := unmarshal(&source)
	if err != nil {
		return err
	}
	return spec.ParseIdent(source)

}

// BuildSpec is a set of structured inputs to build a package
type BuildSpec struct {
	Command  string      `yaml:"command"`
	Options  []OptionMap `yaml:"opts"`
	Variants []OptionMap `yaml:"variants"`
}

// OptionMap is a set of values for package build options
type OptionMap map[string]string

func (om OptionMap) OrderedKeys() []string {

	keys := make([]string, 0, len(om))
	for key := range om {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys

}

func (om OptionMap) Digest() string {

	hasher := sha1.New()
	for _, name := range om.OrderedKeys() {
		hasher.Write([]byte(name))
		hasher.Write([]byte{'='})
		hasher.Write([]byte(om[name]))
		hasher.Write([]byte{0})
	}
	return base32.StdEncoding.EncodeToString(hasher.Sum(nil))[:digestSize]

}
