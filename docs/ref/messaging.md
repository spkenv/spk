---
title: Messaging
summary: Messaging from SPK
weight: 120
---

This documents SPK's support for sending event messages to external messaging systems.

## Messaging from SPK

`SPK` supports sending messages to configured, external, messaging
systems after certain events.

It can send package update messages for package events, when packages are:
- published
- modified (deprecated or undeprecated)
- removed

It can send index status messages for index events, when:
- An index update is started
- An index update is in-progress (periodically as the update is happening)
- An index update is complete
- A SPK Indexer is running (a heartbeat message)

The external messaging system must be set up separately to SPK by the
site. SPK's config file must have a messaging channel configuration
added to it for the messaging system.


### Supported messaging systems

SPK only supports sending messages for events to kafka systems.

To send messages, SPK must be configured for each messaging system the
sites wants it to send to. Multiple messaging channels can be
configured. Each must be configured as a separate SPK message channel.

Currently, kafka is the only supported kind of messaging channel.

### Message channel configuration

A kafka message channel must be configured with:
- a `name`
- a list of `brokers`
- a `package_updates_topic_name`, for package update events
- a `index_updates_topic_name`, for index status update events
- a set of `repo_names` that SPK is allowed to send messages about

There are several other optional configuration settings for a kafka
message channel. See [SPK config file]({{< ref "../admin/config" >}})
for more details.

### Index message channel configuration

An index (for a repository) can be configured so that index update
messages are sent to a particular message channel. Each SPK repository
has an index configuration section and the `update_message_channel`
property can be set to the `name` of the message channel SPK will use
for sending index status updates. 

Index status update messages are sent when an index is being generated
or updated. This is usually done by an Indexer, but can be done with
`spk repo ...` command lines.

How often index status messages are sent can be configured via the
`update_event_send_freq_ms` setting in the index's configuration.

See [SPK config file]({{< ref "../admin/config" >}}) for more details.


### Indexer messaging configuration

An Indexer (index updating service) has to be configured to use a
named messaging channel before it can be used to monitor package
updates for a repository and run index updates.

The these settings must be configured:
- `message_channel_name` of the messaging channel system to use
- `repo_name` of the repository to monitor and index

A messaging channel must exist with that name, and the repository name
must be present in that messaging channel's list of repo names that it
allows. Otherwise the Indexer will not be able to send or receive
messages about the repository it is interested in.

See [SPK config file]({{< ref "../admin/config" >}}) for more details.


## SPK message formats

SPK sends messages in json format. There are two kinds of messages:
1. Package update messages
2. Index status messages

 Each goes to a different topic/queue in a configured messaging system.


### Package update messages

Package update messages contain these fields and data:
```
{
  "event": "generating event"
  "repo": "repository address"
  "repo_name": "repository name"
  "package": "package/version/build identifier"
}
```
For example:
```
{
  "event": "package published",
  "repo": "file:/on/a/file/system/some/where",
  "repo_name": "origin",
  "package": "mypkg/1.2.3/ABCDEF"
}
```
There are four valid package event values:
- "package published"
- "package modified"
- "package removed"
- and the special: "generate index", see [SPK Indexer]({{< ref "./indexes" >}}).


### Index status messages

Index status message come in two broad kinds: index status messages,
and Indexer heartbeat messages. Both use the same message format and
structure, but the heartbeat messages contain placeholder data in some
fields.

```
{
  "event": "index status",
  "repo": "repository address",
  "name": "repository name",
  "start": "the timestamp of when the index started to change"
}
```

The `start` timestamp will be the same for all index status messages
related to the same index update processing. They will all be when
that index update first started. This lets message consumers both:
group related index status changes, and detect which ones are recent
enough to be relevant to them.

For example:
```
{
  "event": "index in-progress",
  "repo": "file:/on/a/file/system/some/where",
  "name": "origin",
  "start": 1777941742
}
``

There are four valid index event values:
- "index started"
- "index in-progress"
- "index completed"
- "indexer heartbeat"

