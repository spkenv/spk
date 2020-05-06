package spm

// Release represents a package release specification
type Release struct{ source string }

// ParseRelease parses a string as a package release specification
func ParseRelease(source string) Release {
	return Release{source}
}

func (release Release) String() string {
	return release.source
}
