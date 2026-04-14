+++
title = "ElastiCache"
description = "Real Redis and Valkey clusters via Docker. Replication groups, snapshots, user/group management."
weight = 18
+++

fakecloud implements **75 of 75** ElastiCache operations at 100% Smithy conformance. Cache clusters run in **real Docker containers** — your code connects to a real Redis or Valkey instance.

## Supported features

- **Cache clusters** — CreateCacheCluster, ModifyCacheCluster, DeleteCacheCluster, DescribeCacheClusters
- **Real engines via Docker** — Redis, Valkey
- **Replication groups** — CreateReplicationGroup with primary/replica topology
- **Global replication groups** — cross-region global datastores (CRUD)
- **Serverless caches** — CreateServerlessCache, ModifyServerlessCache
- **Snapshots** — CreateSnapshot, CopySnapshot, DeleteSnapshot, RestoreReplicationGroupFromSnapshot
- **Serverless cache snapshots** — CRUD
- **Subnet groups** — CRUD
- **Users and user groups** — IAM-integrated auth
- **Parameter groups** — CRUD
- **Security groups** — cache security group CRUD
- **Failover** — TestFailover, TestMigration
- **Tagging** — AddTagsToResource, RemoveTagsFromResource
- **Engine versions** — DescribeCacheEngineVersions

## Protocol

Query protocol. Form-encoded body, `Action` parameter, XML responses.

## How the Docker integration works

When you call `CreateCacheCluster` or `CreateReplicationGroup` for a Redis/Valkey topology, fakecloud starts real Docker containers running the corresponding official image and reports the mapped host port(s). Your application connects with a normal Redis client.

## Gotchas

- **Requires a Docker socket.** ElastiCache needs access to `/var/run/docker.sock`.
- **First use pulls the image.** Expect a slower first run while the Redis/Valkey image downloads.
- **Memcached is not supported via Docker.** Memcached cluster operations conform to AWS (CRUD works) but don't run a real backing process.

## Source

- [`crates/fakecloud-elasticache`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-elasticache)
- [AWS ElastiCache API reference](https://docs.aws.amazon.com/AmazonElastiCache/latest/APIReference/Welcome.html)
