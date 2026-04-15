+++
title = "Persistence"
description = "How fakecloud persists state to disk across restarts."
weight = 2
+++

By default fakecloud keeps all state in memory: startup is instant, shutdown is a no-op, and tests can run in parallel without cross-contamination. That's what you want for CI and most dev workflows.

For longer-running local environments where you want state to survive restarts, pass `--storage-mode=persistent --data-path=<dir>` to mirror supported services to disk.

## Enabling persistent mode

```sh
fakecloud --storage-mode persistent --data-path /var/lib/fakecloud
```

Or via environment:

```sh
FAKECLOUD_STORAGE_MODE=persistent FAKECLOUD_DATA_PATH=/var/lib/fakecloud fakecloud
```

## What's persisted

- **S3** — buckets, objects, versions, delete markers, multipart uploads (resumable across restarts), and every bucket subresource: tags, lifecycle, CORS, policy, notification, logging, website, public access block, object lock, replication, ownership, inventory, encryption, ACL, accelerate. Written to disk on every mutation and reloaded on startup.
- **API Gateway v2** — HTTP APIs, routes, integrations, stages, deployments, authorizers.
- **Every other service** emits `persistence not yet supported, running in-memory` at startup and continues to operate exactly as in memory mode. Your tests don't break — you just don't get cross-restart durability for those services yet.

## Version compatibility

On startup fakecloud reads `<data-path>/fakecloud.version.toml`. The file records the on-disk format version and the fakecloud version that created the directory. If the format version doesn't match the running binary, startup fails with an actionable error that points at the file.

There is no automatic migration — the intent is that you either keep using the binary that wrote the directory or start from an empty data path.

## S3 object body handling

Object bodies are streamed straight to disk in persistent mode, not held in RAM. A bounded LRU cache (`--s3-cache-size`, default 256 MiB) keeps recently read bodies available for fast re-reads. Objects larger than `cache-size / 2` bypass the cache on both read and write, so a single large upload cannot evict the entire working set.

## Introspection buffers are not persisted

The `/_fakecloud/s3/notifications` buffer — and every other `/_fakecloud/*` introspection endpoint — is intentionally not persisted. These exist so tests can assert which events fired during the current run, not as a long-term audit log.
