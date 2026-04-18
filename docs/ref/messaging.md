---
title: Messaging
summary: Messaging from SPK
weight: 120
---

This explains SPK'S support for sending messages to external messaging systems

## Messaging from SPK

`SPK` supports sending messages to configured messaging systems after certain events.

These events are when packages are:
- published
- modified
- deleted


### Supported messaging systems

SPK only supports sending messages for events to kafka systems.


### SPK messaging configuration

To send messages, SPK must be configured for each messaging system it
will send to. Multiple backend messaging systems can be specified.

See [here]({{< ref "../admin/config" >}}) for the kafka configuration.


### SPK message format

SPK sends messages in json format. They contain these fields and data:
```
{
  "event": "generating event"
  "repo": "repository name"
  "package": "package/version/build identifier"
}
```
For example:
```
{
  "event": "package published"
  "repo": "origin"
  "package": "mypkg/1.2.3/ABCDEF"
}
```
