---
title: Indexes
summary: Indexes for improving solve times
weight: 120
---

This explains indexes, indexing, index controls, and index requirements in `spk`.

## Spk Repository Indexes

`SPK` supports generating a packages index for a repository to help
with solves. An index must have been generated separately for a
repository before it can be used. Using an index speeds up solves
against that use that repository.

The index is designed to help the solvers with solves (package
look-ups, deprecation checks, pre-loading non-package variable
references, getting install requirements, etc.). It does not contain
the full package data. So it does not have the information needed to
help other `spk` operations, e.g. building or testing a package.

If indexing is enabled, you have to generate an index before to trying
to use it in a solve. They are not generated on the fly (outside of
automated tests).

If index use is enabled, but no index has been generated for a
repository, `spk` will fallback to using the underlying repository's
packages directly. This typically results in slower solves,
particularly with larger repositories.

If a solve uses multiple repositories and indexes exist for some or
all of them, `spk` will use the indexes that exist.


### Enabling/Disabling Indexes

`spk` index use can be enabled or disabled in the `spk` config file.
It is disabled by default because index generation and updating needs
to be set up on per repository basis. Setting this up is recommended
for repositories with a large number of packages.

Most `spk` commands can use the `--use-indexes` and `--no-indexes`
flags to override repository index use.`spk repo index ...` command
always has index use disabled because it operates on the indexes
themselves. `spk info ...` also disables indexes because to display
full package information requires reading the package from the
repository directly, not from an index.

If index use is enabled in the config file, it can be disabled with
the `--no-indexes` command line flag.

If index use is disabled in the config file, it can be enabled with the
`--use-indexes` flag.

### Generating an Index

To generate an index for a repository (e.g. origin), run:
`spk repo index --disable-repo local`

This will generate a flatbuffer schema based index file. The index
file is stored in the underlying spfs repo (e.g. origin repo) in a
`index/spk/` sub-directory.

Index generation only works on one repository at a time (hence the
`--disable-repo local` when working on the default `origin` repo). If
you have multiple repositories to index you have to run `spk repo index
...` once for each repository.

See `spk repo index -h` for more details.

You don't have to generate a full index every time a package changes,
you can also update a package in an existing index, see the next
section.


### Updating an existing Index

Updating a package in an existing index, such as after a new build is
published or a package is deprecated, is not automatic.

A site using `spk` has to set up a system to trigger index updates
when they want them to happen. The recommendation is after a new
package is published, deprecated, undeprecated, or deleted. But
regular periodic complete index generation may also work for a site,
depending on the frequency of package changes and the periodic updates.

To update an existing index, e.g. after a new `python` package was
published, run:
`spk repo index --disable-repo local --update python`

This will read in the existing index for the repository and update the
versions and builds of the named package in the index. It is faster
than generating an index from scratch. It has to be run once per
repository and once per package.

See `spk repo index -h` for more details.


## Index vs Repository mismatches - updates are important

When index use is enabled, it is important to update the indexes when
changes happen to the repository and its packages. Otherwise, the
index and real package data will get out of sync. An index that is old
won't have the same data as the repositories it is based one. This can
lead to discrepancies in solves.


## Index Details

### Index file location

An index is stored in a file inside the repository it indexes. Only
spk filesystem repositories (based on spfs fs repositories) support
indexes. The file will be kept in the `index/spk/` sub-directory of an
spfs fs repository.


### Structure and types in SPK

An index is implemented by a set of types in spk-storage and
spk-schema that work together to wrap a repository in its index and
produce indexed packages that act as packages for the solver.
 
The hierarchy of index related types:

```
in the 'spk-storage' crate:

RepositoryHandle enum
   Indexed struct (implements Repository and Storage traits)
       RepoIndex enum (implements RepoIndex and RepoIndexMut traits)
           FlatBufferRepoIndex struct (implements RepoIndex and RepoIndexMut traits)
               flatbuffers schema

in the 'spk-schema' crate:
   IndexedPackage (implements Package traits, produced by a FlatBufferRepoIndex)
       flatbuffers schema
       
```



### Flatbuffer index schema

`spk` uses flatbuffers for the index data format on disk. This is fast
to read and use, but slow to generate. Updates to an existing index
require an index to be read in completely and written out again with
the updated data. It does not support updating complex structures
in-place.

The `spk-proto` crate contains spk's flatbuffers schema for an
index. It requires `flatc` to be installed to generate the rust code
for the index schema.

The schema is versioned internally, and in the index file name contains
the schema version number as well.

The index structure does not match the rust object structures
1-to-1. The is partially due to only keeping things needs for solves,
and partially due to what flatbuffers require, support, and recommend
(lists not sets or mappings, removing duplication and intermediate
structures).

The broad top-level schema structure is (capital letters indicate a schema table name):
```
RepositoryIndex
   schema version number
   list of PackageIndex
              name
              list of VersionIndex
                          version number
                          list of BuildIndex
                                      build digest/id
                                      ... other fields needed for solving
   list of GlobalVar  (used to prime the 'resolvo' solver to avoid restarts)
               name
               list of valid values
```

The structure is quite deep to cover the data for options,
requirements, compat rules, version numbers, build ids, and
components. See the schema itself for more detailed information,
including which fields of rust objects are being ignored, omitted from
an index.


### How to evolve the index schema

The flatbuffers data format supports adding new fields and structures
without breaking existing code, provided fields are not deleted. New
fields should be added only to the ends of tables.

The index schema's version number lets `spk` check that the index data
is compatible with the current spk being run. That is, that the code
understands this version of the schema. There will be situations where
multiple versions of spk are being used in a site and they may not all
be able to use index data generated by other versions of spk.

Spk includes the schema version number in the index file name to allow
multiple index schema to co-exist during a transition between index
versions.

#### Adding a new field

When a new field needs to be added to the index schema, the developer should:
- Increment the schema version number
- Add the new field to the schema
- Increment spk's compatible schema version number
- Add spk code to read and write the new field
- Make a new release of spk
- Generate a new index using the new spk build
- Transition the site to the new spk release
- Once the transition is complete, retire the older index data file and generation

During the transition the older version of spk will use the older
index format(s) and the newer spk will use the newer one. Both indexes
will have to be generated and kept up to date during the transition.
Once the older spk version is fully retired, the older index data can
be retired too.


#### Stop using an existing field

When an older field should not be used anymore, the developer should:
- Add spk code to stop reading the old field
- Make a new release of spk that no longer reads the field
- Transition the site to the new spk release

There's no need to change the index schema version for to stop reading
a field, and index generation remains unchanged (even though it
includes data that isn't read by spk anymore),


#### Removing an old field

When a new field needs to be removed from the index schema, the developer should:
- Double check the reason for removing the field, it may not be worth the trouble. This is a 2 stage process:
- Stage 1:
   - Add spk code to stop reading the old field
   - Make a new release of spk that no longer reads the field
   - Transition the site to the new spk release
- Stage 2:
   - Increment the schema version number
   - Increment spk's compatible schema version number
   - Add spk code to stop writing the old field
   - Make another new release of spk that no longer writes the field
   - Transition the site to the new spk release
   - Once that transition is complete, retire the older index data file and generation

During the transition the older version of spk will use the older
index format(s) and the newer spk will use the newer one. Both indexes
will have to be generated and kept up to date during the transition.
Once the older spk version is fully retired, the older index data can
be retired too.
