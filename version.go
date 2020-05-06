package spm

// Version specifies a package version number
type Version struct{ source string }

// ParseVersion parses a string as a version specifier
func ParseVersion(source string) Version {
	return Version{source}
}

func (version Version) String() string {
	return version.source
}
