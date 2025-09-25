---
title: Package Sources
summary: Pulling in code or other build inputs to generate a source package.
weight: 20
---

The `sources` section of the package spec tells spk where to collect and how to arrange the source files required to build the package. Currently, it defaults to collecting the entire directory where the spec file is loaded from, but can be overridden with a number of different sources.

### Local Source

Local directories and files are simply copied into the source area. Paths here can be absolute, or relative to the location of the spec file. Git repositories (`.git`) and other source control files are automatically excluded, using the rsync `--cvs-exclude` flag. Furthermore, if a `.gitignore` file is found in the identified directory, then it will be used to further filter the files being copied.

```yaml
sources:
  # copy the src directory next to this spec file
  - path: ./src
  # copy a single file from the config directory
  # into the root of the source area
  - path: ./config/my_config.json
```

### Git Source

Git sources are cloned into the source area, and can take an optional ref (tag, branch name, commit) to be checked out.

```yaml
sources:
  - git: https://github.com/qt/qt5
    ref: v5.12.9
```

### Tar Source

Tar sources can reference both local tar files and remote ones, which will be downloaded first to a temporary location. The tar file is extracted automatically into the source area for use during the build.

```yaml
sources:
  - tar: https://github.com/qt/qt5/archive/v5.12.9.tar.gz
```

### Script Source

Script sources allow you to write arbitrary bash script that will collect and arrange sources in the source package. The script is executed with the current working directory as the source package to be built. This means that the script must collect sources into the current working directory.

Any previously listed sources will already exist in the scripts current directory, and so the script source can also be used to arrange and adjust source files fetched through other means.

```yaml
sources:
  - script:
      - touch file.yaml
      - svn checkout http://myrepo my_repo_svn
```

### Multiple Sources

You can include sources from multiple locations, but will need to specify a subdirectory for each source in order to make sure that they are each downloaded/fetched into their own location in the source package. Some sources can be intermixed into the same location (such as local sources) but others require their own location (such as git sources).

```yaml
sources:
  # clones this git repo into the 'someproject' subdirectory
  - git: https://github.com/someuser/someproject
    ref: main
    subdir: someproject
    # copies the contents of the spec file's location into the 'src' subdirectory
  - path: ./
    subdir: src
```
