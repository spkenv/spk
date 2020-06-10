---
title: Resolving Environments
summary: Understanding the solver and dealing with issues.
weight: 100
---

The `spk env` creates a brand new environment with some set of packages installed into it. Once you are in an environment, the `spk install` command can be used to add additional packages or upgrade existing packages in the current environment.

Both of these operations take a set of package requests and try to figure out the best way to satisfy them all (more info on [package requests](../versioning)). The solver is responsible for taking the set of requested packages and ensuring that all dependencies are pulled in and all packages are compatible in the final environment. If this is deeped not possible, then you will see an error related to why the requests could not be satisfied.

## Understanding Solver Errors

Depending on the complexity of the requests and number of dependencies of each package, the final error that you see is not always the most useful one. There are a number of ways that you can try to understand what went wrong which can give you insight into possible fixes. The best place to start is the `spk explain` command, which takes the same set of package requests and prints out the decision tree of the solver. This output can be quite verbose, but often provides much better insight into what went wrong.

To help understand the decision tree and what can go wrong let's look at some examples:

### Package Doesn't Exist

```bash
$ spk explain doesntexist
REQUEST doesntexist/*
> BLOCKED Package not found: doesntexist
```

This error is one of the most obvious - but knowing when and why it can appear helps with understanding other issues. This error happens when the package that was requested simple doesn't exist in and of the enabled repositories.

#### Possible Solutions

- check that the package name was spelled correctly
- if you just created the package but haven't published it, make sure the the `--local-repo` flag is used
- if the package is in a testing or other alternative repository, make sure to enable the repository with `--enable-repo=<name>`
- try using the `spk search` command to see if the package lives under a slightly different name

### No Applicable Versions

```bash
$ spk explain gcc/3.*
 REQUEST gcc/3.*
> BLOCKED Failed to resolve: {'pkg': 'gcc/3.*'} - from versions: []
```

In this case, the package exists but still failed to resolve. The lack of versions in this error message doesn't mean that there are no version of the package available (because that would cause a _package not found error_ instead). The lack of versions means that none of the available versions were even worth looking at.

#### Additional Information

The solver doesn't know for sure if a package is compatible until it loads the spec file, but it will also only load versions that are not obviously incomatible. In this case, our use of a wildcard version means that anything version starting with something other than 3 is never going to be relevant and there are no gcc packages with a major version number of 3.

#### Possible Solutions

- if you just created a new version but haven't published it, make sure the the `--local-repo` flag is used
- if the package is in a testing or other alternative repository, make sure to enable the repository with `--enable-repo=<name>`
- try using the `spk ls <name>` command to list the available versions of the package
- try loosening the version requirements or using a different version of the requested package altogether

### Solver Patterns

In most cases, the solver will encouter multiple issues as it tries to find an appropriate solution to the set of requested packages. Not all errors are bad, and some are even expected. Here are some common patterns that you may see in your solver decision tree which can help to differentiate benign issues from actual problems.

```bash
$ spk env my-plugin maya/2020
REQUEST my-plugin/* maya/2020.0.0
> RESOLVE my-plugin/10.0.0/3I42H3S6 REQUEST maya/2019.0.0
>> BLOCKED Failed to resolve: {'pkg': 'maya/2020.0.0,2019.0.0'} - from versions: [2019.2.0, 2020.2.1]
> BLOCKED Failed to resolve: {'pkg': 'my-plugin/*'} - from versions: [10.0.0]
```

#### Incompatible Dependencies

```bash
$ spk explain my-plugin/1, maya/2020
REQUEST my-plugin/1.0.0, maya/2020.0.0
> RESOLVE my-plugin/1.0.0/3I42H3S6 REQUEST maya/2019.0.0
>> BLOCKED Failed to resolve: {'pkg': 'maya/2019.0.0,2020.0.0'} - from versions: [2020.0.0]
> BLOCKED Failed to resolve: {'pkg': 'my-plugin/1.0.0'} - from versions: [1.0.0]
```

In this example, we've requested my-plugin version 1 and maya 2020. The solver resolved my-plugin/1.0.0 but this package has it's own dependency on maya/2019, as denoted by the additional REQUEST which is added. The solver combines the two requests into one, and then cannot find a version that satisfies both the `2019` and `2020` request.

The last line of this output is the unwinding of the solver stack. When the first error happens, the solver steps back and tries to see if there is another version of `my-plugin` that can be used (hopefully without the conflicting dependency). In this case there are none left and so it fails.

#### Recovered Incompatibility

```bash
$ spk explain my-plugin/1 maya/2019
 REQUEST my-plugin/1.0.0, maya/2019.0.0
> RESOLVE my-plugin/1.1.0/3I42H3S6 REQUEST maya/2020.0.0
>> BLOCKED Failed to resolve: {'pkg': 'maya/2019.0.0,2020.0.0'} - from versions: [2020.0.0]
> RESOLVE my-plugin/1.0.0/3I42H3S6 REQUEST maya/2019.0.0
>> RESOLVE maya/2019.0.0/3I42H3S6
```

Similar to above, `my-plugin/1.1.0` has a dependency on `maya/2020` which conflicts with the original request for maya 2019. In this case, the solver backed out and tried an older version of `my-plugin`, which requested maya 2019 instead and so the resolve was completed.

#### Revisiting a Request

```bash
$ spk explain my-plugin
 REQUEST my-plugin/*
> RESOLVE my-plugin/1.0.0/3I42H3S6 REQUEST maya/2019.0.0, some-library/1.0.0
>> RESOLVE maya/2019.2.0/3I42H3S6
>>> RESOLVE some-library/1.0.0/3I42H3S6 REQUEST maya/~2019.0.0 UNRESOLVE maya
>>>> RESOLVE maya/2019.0.0/3I42H3S6
```

In this example, `my-plugin` has two dependencies. The first maya dependency is resolved to `2019.2` but then when `some-library` is resolved, it adds a new request for `maya/~2019.0.0` for which `2019.2` is not applicable. The solver re-opens the request, denoting this by the UNRESOLVE statement above, and then manges to find an older version of maya that works for both requests.
