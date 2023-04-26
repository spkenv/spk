---
title: Resolving Environments
summary: Understanding the solver and dealing with issues.
weight: 100
---

The `spk env` creates a brand new environment with some set of packages installed into it. Once you are in an environment, the `spk install` command can be used to add additional packages or upgrade existing packages in the current environment.

Both of these operations take a set of package requests and try to figure out the best way to satisfy them all (more info on [package requests](../versioning)). The solver is responsible for taking the set of requested packages and ensuring that all dependencies are pulled in and all packages are compatible in the final environment. If this is deeped not possible, then you will see an error related to why the requests could not be satisfied.

## Understanding Solver Errors

Depending on the complexity of the requests and number of dependencies of each package, the final error that you see is not always the most useful one. There are a number of ways that you can try to understand what went wrong which can give you insight into possible fixes. The best place to start is the `spk explain` command, which takes the same set of package requests and prints out the decision tree of the solver. This output can be quite verbose, but often provides much better insight into what went wrong. This output can also be retrieved and further expanded by specifying the `--verbose (-v)` flag a number of times (eg `spk env -vvv my-package/1`)

To help understand the decision tree and what can go wrong let's look at some examples:

### Package Doesn't Exist

```console
$ spk explain -v doesntexist
 DEFINE {arch=x86_64, centos=7, distro=centos, os=linux}
 REQUEST doesntexist/*
!BLOCKED Package not found: doesntexist
```

This error is one of the most obvious - but knowing when and why it can appear helps with understanding other issues. This error happens when the package that was requested simple doesn't exist in and of the enabled repositories.

#### Possible Solutions

- check that the package name was spelled correctly
- if you just created the package but haven't published it, make sure the the `--enable-repo=local` flag is used
- if the package is in a testing or other alternative repository, make sure to enable the repository with `--enable-repo=<name>`
- try using the `spk search` command to see if the package lives under a slightly different name

### No Applicable Versions

```console
$ spk explain -v gcc/3.*
 REQUEST gcc/3.*
 DEFINE {arch=x86_64, centos=7, distro=centos, os=linux}
 REQUEST gcc/3.*
. TRY gcc/6.3.1 - Out of range: 3.* [at pos 0]
. TRY gcc/4.8.5 - Out of range: 3.* [at pos 0]
!BLOCKED failed to resolve 'gcc'
```

In this case, the package exists but still failed to resolve. We can see that the solver looked at the existing versions of the `gcc` package but found that none of them were applicable to the requested version range.

#### Additional Information

In these cases, the additional use of the `--verbose (-v)` flag is extremely helpful, as it shows us that the solver tried to find an appropriate version and, most importantly, it shows us why none of those versions could be used.

#### Possible Solutions

- if you just created a new version but haven't published it, make sure the the `--enable-repo=local` flag is used
- if the package is in a testing or other alternative repository, make sure to enable the repository with `--enable-repo=<name>`
- try using the `spk ls <name>` command to list the available versions of the package
- try loosening the version requirements or using a different version altogether of the requested package

### Incompatible Options

```console
$ spk explain -v gcc/6 -o os=darwin
 DEFINE {arch=x86_64, centos=7, distro=centos, os=darwin}
 REQUEST gcc/6.0.0
. TRY gcc/6.3.1 - invalid value for os: Invalid value 'darwin' for option 'os', must be one of {'linux'}
. TRY gcc/4.8.5 - Not compatible with 6.0.0 [x.a.b at pos 0]
!BLOCKED failed to resolve 'gcc'
```

In this example, we've specifically requested an environment where the `os` option is `darwin`. We can see by the different error message that although there is a `gcc/6.3.1` package available that it was build for `os: linux`, which is not what we requested.

#### Possible Solutions

- Ensure that all your options are appropriate for the package that you are requesting
- Consider whether the build options that you are using are required or unnecessarily specific
- Build or request a build of the package with the necessary options
- Request a different version of the package which has a build available for the desired options

### Solver Patterns

In most cases, the solver will encouter multiple issues as it tries to find an appropriate solution to the set of requested packages. Not all errors are bad, and some are even expected. Here are some common patterns that you may see in your solver decision tree which can help to differentiate benign issues from actual problems.

#### Incompatible Dependencies

```console
$ spk explain -v my-plugin/1, maya/2020
 REQUEST my-plugin/1.0.0
 REQUEST maya/2019.0.0
> RESOLVE my-plugin/1.0.0/3I42H3S6
. REQUEST maya/2020.0.0
.. TRY maya/2020.0.0/3I42H3S6 - Not compatible with 2019.0.0 [x.a.b at pos 0]
.. TRY maya/2019.0.0/3I42H3S6 - version too low
!!BLOCKED failed to resolve 'maya'
!BLOCKED failed to resolve 'my-plugin'
```

In this example, we've requested my-plugin version 1 and maya 2020. The solver resolved `my-plugin/1.0.0` but this package has it's own dependency on `maya/2019`, as denoted by the additional `REQUEST` which is added. The solver combines the two requests into one, and then cannot find a version that satisfies both the `2019` and `2020` request.

The last line of this output is the unwinding of the solver stack. When the first error happens, the solver steps back and tries to see if there is another version of `my-plugin` that can be used (hopefully without the conflicting dependency). In this case there are none left and so it fails.

#### Recovered Incompatibility

```console
$ spk explain -v my-plugin/1 maya/2019
 REQUEST my-plugin/1.0.0
 REQUEST maya/2019.0.0
> RESOLVE my-plugin/1.1.0/3I42H3S6
. REQUEST maya/2020.0.0
.. TRY maya/2020.0.0/3I42H3S6 - Not compatible with 2019.0.0 [x.a.b at pos 0]
.. TRY maya/2019.0.0/3I42H3S6 - version too low
!! BLOCKED failed to resolve 'maya'
> RESOLVE my-plugin/1.0.0/3I42H3S6
. REQUEST maya/2019.0.0
.. TRY maya/2020.0.0/3I42H3S6 - Not compatible with 2019.0.0 [x.a.b at pos 0]
>> RESOLVE maya/2019.0.0/3I42H3S6
```

Similar to above, `my-plugin/1.1.0` has a dependency on `maya/2020` which conflicts with the original request for maya 2019. In this case, the solver backed out and tried an older version of `my-plugin`, which requested maya 2019 instead and so the resolve was completed.

#### Revisiting a Request

```console
$ spk explain -v my-plugin
 REQUEST my-plugin/*
> RESOLVE my-plugin/1.0.0/3I42H3S6
. REQUEST maya/2019.0.0
. REQUEST some-library/1.0.0
>> RESOLVE maya/2019.2.0/3I42H3S6
... TRY some-library/1.0.0/3I42H3S6 - Conflicting install requirement: 'maya' version too high
!!! BLOCKED failed to resolve 'some-library'
>> RESOLVE maya/2019.0.0/3I42H3S6
>>> RESOLVE some-library/1.0.0/3I42H3S6
... REQUEST maya/~2019.0.0
```

In this example, `my-plugin` has two dependencies. The first maya dependency is resolved to `2019.2` but then when `some-library` is resolved, it adds a new request for `maya/~2019.0.0` for which `2019.2` is not applicable. Similar to above, the solver steps back and tries again with an older version of maya which ends up being applicable to both requirements.

#### Deprecated Packages

```console
$ spk explain -v my-tool
 REQUEST my-tool/*
. TRY my-tool/1.2.0/STLY6HNC - Build is deprecated and was not specifically requested
. TRY my-tool/1.2.0 - Package version is deprecated
! BLOCKED failed to resolve 'my-tool'
```

Packages can be deprecated by package owners when an issue is found or an older version is no longer fit for use. Deprecated packages should not be used under normal circumstances, but there are ways to use the packages if absolutely required.

##### Possible Solutions

- Generally, you want to update to a newer version of the package that has not been deprecated. Package maintainers should not deprecate packages without providing a resonable alternative.
- If you are really stuck, note that the error message says _was not specifically requested_. This means that if you request the deprecated build exactly, then it will still resolve the environment for you, eg `spk env my-tool/1.2.0/STLY6HNC`.

#### Embedded Packages

Some packages, especially DCC packages, are bundled with other software/packages. Package maintainers should include these packages as _embedded_ packages, so that the solver understands what's in the bundle. The solver will show embedded packages being requested and resolved, always with the `embedded` build string.

```console
$ spk explain qt maya
 REQUEST qt/*
 REQUEST maya/*
> RESOLVE qt/5.13.0/3I42H3S6
!! BLOCKED Package maya embeds package already resolved: qt
.. REQUEST maya/*
> RESOLVE maya/2019.2.0/3I42H3S6
. REQUEST qt/=5.12.6/embedded
. RESOLVE qt/5.12.6/embedded
```

In this case, `qt` was resolved to version 5.13 first, but it blocked `maya` from being resolved, since `maya` brought in its own embedded version of `qt`. The solver backtracks to before `qt` was resolved to try a different path. It resolves the `maya` package with its embedded `qt`, which satisfies the original request for both `qt` and `maya`. The solver will always show the same `RESOLVE` message for embedded packages, but embedded packages can only ever resolve to the one bundled with the package in question.
