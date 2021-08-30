# Design and Architecture

General concepts and codebase layout.

## General Concepts

Conceptually, spfs has a few distinct layers to its design which are also mostly represented in the shape of the codebase.

{{< mermaid >}}
graph LR;
spfs[SpFS] --> storage
spfs --> tracking
storage[Storage] --> graf[Graph]
tracking[Tracking] --> graf
graf --> encoding[Hashing and Binary Encoding]
{{< /mermaid >}}

### Object Graph

At the very heart of spfs is a directional acyclical graph (DAG) that contains and connects all of the filesytem data that spfs tracks. Each node in the graph is one of the core object types in spfs. Although the implementation supports any DAG structure, in practice, the objects are easier to picture in a tree structure:

{{< mermaid >}}
graph LR;
platform -->|many| layer
layer -->|one| manifest
manifest -->|one| tree
tree -->|many| tree
tree -->|many| blob
{{< /mermaid >}}

Each of these objects plays a key role in how spfs tracks filesystem data.

- **platform** - A platform specifies any number of layers stacked in a specific order.
- **layer** - A filesystem layer whose contents are identified by a single contained manifest.
- **manifest** - Represents a complete filesystem tree (all directories and files).
- **tree** - A single directory, with any number of contained entries which may be other directories or files.
- **blob** - An arbitrary container of opaque data, aka a file in the filesystem.

### Object Hashing and Encoding

All objects in the spfs graph API are binary encodable. They can all be deterministically written to and read from a binary representation. This binary form of each object is what spfs uses in order to hash and determine the unique digest/identifier for the object.

SpFS uses the **sha256** algorithm to hash objects, and a **base32** encoding of that digest when referring to the digest in a human-readable string.

These digests are used throughout spfs to uniquely identify and refer to objects.

### Object Tracking

The spfs tracking module is concerned with populating object graphs from real filesytem data, providing human-friendly identification for important data within a graph, and comparing data stored within a graph.

The most important concept introduced here is the `tag` which connects a human-readable name to an object in the graph. Tags are stored in a _stream_ which gives them a timestamped history.

### Object Storage (Repositories)

Being able to represent all of its data as a connected graph of objects and identify them with a unique digest is key to the logic of spfs, but the object data needs to be persisted somewhere in order to be useful for users. For this reason, spfs defines an interface through which all object data can be saved and loaded.

As of writing, spfs provides two implementations of the storage api, a local filesystem store and a tar archive store. Note that the tar archive works by unpacking and using a filesystem store at runtime, repacking the tar archive when complete.

Initializing a filesystem repository that can be used as your "origin" remote currently requires that you manually create the repository directory structure on disk when using `file:...` repositories.

```bash
mkdir -p /path/to/remote/spfs-storage/{objects,payloads,tags}
```

### SpFS

Above these core layers sits the main spfs API layer, which provides high level functions that deal with syncing data between two repositories, cleaning orphaned objects from a repository, creating new layers from an active filesystem, and orchestrating with the runtime environment.
