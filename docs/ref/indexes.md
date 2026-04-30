---
title: Indexes
summary: Indexes for improving solve times
weight: 120
---

This explains indexes, indexing, index controls, and index requirements in `SPK`.

## SPK Repository Indexes

`SPK` supports generating a packages index for a repository to speed
up solves. An index must have been generated separately for that
repository before it can be used. 

The index is designed to help the solvers with solves (package
look-ups, deprecation checks, pre-loading non-package variable
references, getting install requirements, etc.). It does not contain
the full package data. So it does not have the information needed to
help other `SPK` operations, e.g. building or testing a package.

If indexing is enabled, the index must be generated before trying to
use it in a solve. They are not generated on the fly (outside of tiny
repositories for automated tests).

If index use is enabled, but no index has been generated for a
repository, `SPK` will fallback to using the underlying repository's
packages directly. This typically results in slower solves,
particularly with larger repositories.

If a solve uses multiple repositories and indexes exist for some or
all of them, `SPK` will use the indexes that exist.


### Enabling/Disabling Indexes

`SPK` index use can be enabled or disabled in the `SPK` config file.
It is enabled by default but require index generation and index
updating to be set up, on a per repository basis, before it will have
an impact. Setting this up is recommended for repositories with a
large number of packages.

Most `SPK` commands can use the `--index-use <value>` option to override
repository index use on a per command basis. The `spk repo index ...`
command always has index use disabled because it operates on the
indexes themselves. `spk info ...` also disables indexes because
displaying complete package information requires reading the package
from the repository directly, not just the subset of data in an index.

If index use is enabled in the config file, it can be disabled with
the `--index-use disabled` command line option.


### Generating an Index

To generate an index for a repository (e.g. origin), you can run:
`spk repo index -r origin`

This will generate a flatbuffer schema based index file. The index
file is stored in the underlying spfs repo (e.g. origin repo) in a
`index/spk/` sub-directory.

Index generation only works on one repository at a time. If you have
multiple repositories to index you have to run `spk repo index ...`
once for each repository.

See `spk repo index -h` for more details.

You can also set up a SPK Indexer, if you also have a SPK message
channel configured. See the 'Automatic index updates ...' sections
below.

You do not have to generate a full index every time a package changes,
you can update a package in an existing index, see the next section.


### Index vs Repository mismatches - updates are important

When index use is enabled, it is important to update the indexes when
changes happen to the repository and its packages. Otherwise, the
index and real package data will get out of sync. An index that is old
will not have the same data as the repositories it is based on. This
can lead to discrepancies in solves.


### Updating an existing Index

Updating a package in an existing index, such as after a new build is
published or a package is deprecated, is not automatic.

A site using `SPK` has to set up a system to trigger index updates
when they want them to happen. The recommendation is after a new
package is published, deprecated, undeprecated, or deleted. But
regular periodic complete index generation may also work for a site,
depending on the frequency of package changes and the periodic updates.

To update an existing index, e.g. the `origin` repo's index after a
new `python` package was published or deprecated, run:
`spk repo index -r origin --update python`

The `--update` option can be given multiple times to update several
packages at a once, e.g.:
`spk repo index -r origin --update python --update zlib`

The `--update` option can also take a package/version. This lets the
update be restricted to a specific version of a package. This can make
for shorter update times for packages with large numbers of versions,
or builds per version, e.g.:
`spk repo index -r origin --update python/3.10.8 --update zlib/1.2.12`

These commands will read in the existing index for the repository, and
update the versions and builds of the named package in the index. This
is faster than generating an index from scratch. But it has to be run
once per repository to update the give packages in that repository's
index.

See `spk repo index -h` for more details.

You can also set up a SPK Indexer to update a repositories index when
a package changes, if you also have a SPK message channel configured.
See the 'Automatic index updates ...' sections below.

### Automatic index updates when published or modifying packages

SPK has support for automatically updating a repository's index when a
package is published, or modified (e.g. deprecated), if there is an
external messaging channel configured for SPK to talk to.

SPK current only supports kafka as a messaging channel.

With a messaging channel configured, SPK will be able to send package
update messages (to a topic/queue) when any of these spk command are used:
- `spk publish`
- `spk deprecate`
- `spk undeprecate`
- `spk remove`

This allows an external monitoring system to see package updates and
act on them, e.g. by triggering an index update.

SPK's `spk repo indexer --name ...` command can be used to run such a
monitoring system that works with SPK's configured messaging channels
(e.g. `spk repo indexer --name my_indexer_config_name`, see `spk repo
indexer -h` for more options).

An SPK Indexer will run forever and listen to the package update
messages for a single repository. It wil accumulate the package
updates, and when there is a pause in messages, it will perform an
index update on that repository's index, and then wait for more
package update messages.

While it is running, the SPK indexer will send heartbeat messages (to
a topic/queue) to indicate it is alive. While it is updating an index,
the SPK indexer will send index status update messages to indicate
when an index update has started, is in-progress, and has completed.

The SPK commands that send package update messages will also wait and
listen for index status update messages. They do this after they have
finished publishing or updating the packages they were acting on, and
only if a message channel is configured for their use.

Those SPK commands use the index status update messages to workout
when that repository's index update is complete, and then they return
control to the user.  This allows a site to ensure package updates are
reflected in an index before a user's next spk command is run. The
indexer's heartbeat and index update messages are used to detect if
the indexer isn't running so the commands do not wait forever.

See the [SPK messaging docs]({{< ref "./messaging" >}}) and the [SPK
config file reference docs]({{< ref "../admin/config" >}}) for more
details.


#### SPK Indexer

The SPK Indexer is meant to be run continuously as a service to
monitor one repository and update that repository's index. If you have
multiple repositories, and you want to automatically update all their
indexes, you will need to config and run multiple SPK Indexers - each
with their own config name and section.

It is up to each site to deploy the SPK Indexer(s) as a service in the
most appropriate way for that site.

The SPK Indexer will make an index from scratch if one does not exist
for the repository it is monitoring. This allows an Indexer to be used
to bootstrap an index for a repository, and recover an index if it is
removed or another problem occurs.

The special admin "generate a full index" package update message can
be sent to make a SPK Indexer regenerate a repository's index from
scratch, even if one exists already.

See the [SPK messaging docs]({{< ref "./messaging" >}}) and the [SPK
config file reference docs]({{< ref "../admin/config" >}}) for more
details.


#### SPK update commands and index status messages

These SPK commands send package update messages (to a topic/queue)
when they run:
- `spk publish`
- `spk deprecate`
- `spk undeprecate`
- `spk remove`

They also listen to index status messages (from a topic/queue), and
wait for their repository's index to update before the commands
finish. Messages about other repositories' indexes are ignored.

However, because these commands are typically run by users, the cannot
wait forever. The first thing they do is check the latest message (in
the topic/queue) and to see if it is recent enough, e.g. within the
last minute or so. A recent enough index status message means that a
SPK Indexer is active and sending messages. In this case the commands
will continue to listen and wait for index updates. If that message is
too old, the command assumes the indexer is down, that the index is
not going to be updated soon, and it exits immediately rather than
wait.

Provided they get recent enough messages, the commands will check the
`index_start_time` field of the messages. They are looking for times
that are after the command finished its updates (e.g. `spk publish`
has published all its packages). Those index updates will include the
package changes made by the command. When the command gets an 'Index
completed' message with a late enough index start time, it knows the
index contains its changes and it will stop listening and can exit
successfully.


## Index Details

SPK supports file-based flatbuffer formatted indexes.

### Index File Location

An index is stored in a file inside the repository it indexes. Only
SPK filesystem repositories (based on spfs fs repositories) support
indexes. The file will be kept in the `index/spk/` sub-directory of an
spfs fs repository.


### Structure and Types in SPK

An index is implemented by a set of types in the spk-storage and
spk-schema crates that work together to wrap a repository with its
index and produce indexed package objects that act as packages for the
solver.
 
The hierarchy of index related types is:

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



### Flatbuffer Index Schema

`SPK` uses flatbuffers for the index data format on disk. This is fast
to read and use, but slow to generate. Updates to an existing index
require an index to be read in completely and written out again with
the updated data. It does not support updating complex structures in
an index in-place

The `spk-proto` crate contains SPK's flatbuffers schema for an
index. It requires `flatc` to be installed to generate the rust code
for the index schema.

The schema is versioned internally, and the index file name contains
the index schema version number as well. This is checked on load
against the index version SPK is compatible with to schema and SPK
version mismatches.

The index structure does not match the rust object structures
one-to-one. This is partially due to only keeping things needed for
solves and solver operations, and partially due to what flatbuffers
require, support, and recommend for speed (lists not sets or mappings,
removing duplication and intermediate structures).

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
components. See the schema file itself for more detailed information,
including which fields of rust objects are being ignored, omitted from
an index.


### How to Evolve the Index Schema

The flatbuffers data format supports adding new fields and structures
without breaking existing code, provided fields are not deleted. New
fields should be added only to the ends of tables.

The index schema's version number lets `SPK` check that the index data
is compatible with the current `SPK` being run. That is, that the code
understands this version of the schema. There will be situations where
multiple versions of `SPK` are being used in a site and they may not all
be able to use index data generated by other versions of `SPK`.

`SPK` includes the schema version number in the index file name to allow
multiple index schemas to co-exist during a transition between index
versions.

#### Adding a New Field

When a new field needs to be added to the index schema, the developer should:
- Increment the schema version number
- Add the new field to the schema
- Increment `SPK`'s compatible schema version number
- Add SPK code to read and write the new field
- Make a new release of SPK
- Generate a new index using the new SPK
- Transition the site to the new SPK release
- Once the transition is complete, retire the older index data file and generation

During the transition the older version of SPK will use the older
index format(s) and the newer SPK will use the newer one. Both indexes
will have to be generated and kept up to date during the transition.
Once the older SPK version is fully retired, the older index data can
be retired too.


#### Stop Reading an Existing Field

When an older field does not need to be read anymore, the developer should:
- Add SPK code to stop reading the old field
- Make a new release of SPK that no longer reads the field
- Transition the site to the new SPK release

There is no need to change the index schema version for to stop reading
a field, and index generation remains unchanged (even though it
includes data that is not read by SPK anymore).


#### Stop Populating an Existing Field

If an older field no longer needs to be populated, the developer should:
- Check the field is no longer read (see above). If it is still being read then continuing may cause unexpected results.
- Increment the schema version number
- Increment SPK's compatible schema version number
- Add SPK code to write None (or the default enum entry depending on the field type) into the index
- Make a new release of SPK that contains this change
- Transition the site to the new SPK release

Technically, the field will still be present in the index schema, and
be set in index generation because it is needed in the index's object
constructors. But the None values will reduce the space the field
takes up. And while this kind of change in backwards compatible, the
empty field may be treated a default value by older versions of SPK
when read from the index - thus the need to version up the index
schema to ensure this does not cause unexpected behaviour when
multiple versions of SPK are in use.


#### Removing an Old Field

Removing an old field will break backwards compatibility of the
flatbuffers format. It is not something that should be done lightly.

When a new field needs to be removed from the index schema, the developer should:
- Double check the reason for removing the field, it may not be worth the trouble. This is a 2 stage process:
- Stage 1:
   - Add SPK code to stop reading the old field
   - Make a new release of SPK that no longer reads the field
   - Transition the site to the new SPK release
- Stage 2:
   - Increment the schema version number
   - Increment SPK's compatible schema version number
   - Add SPK code to stop writing the old field at all
   - Make another new release of SPK that no longer writes the field
   - Transition the site to the new SPK release
   - Once that transition is complete, retire the older index data file and generation process

During the transition the older version of SPK will use the older
index format(s) and the newer SPK will use the newer one. Both indexes
will have to be generated and kept up to date during the transition.
Once the older SPK version is fully retired, the older index data and
its generation can be retired too.
