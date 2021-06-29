# Repository Migrations

Mechanisms for changing filesystem repository formats.

The `spfs migrate` command was ported from the original python implementation, and uses the `spfs::storage::fs::migrations` module to handle upgrading repository data on disk. This mechanism was used once in python but the migration itself was not ported to rust.

The general procedure is to capture as much of the codebase as needed in a subdirectory of the `migrations` module to be able to read the current (soon to be old) version of the repository data. The migrations module contains a global map of available migrations and will handle migrating older repositories through as many versions as necessary to get it up to date.

By default, repositories are required to be migrated only for new **major** versions of the spfs codebase. Regardless of anything else, breaking changes to the storage structure or format MUST NOT be introduced in any other version number change.
