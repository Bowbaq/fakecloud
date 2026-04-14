+++
title = "Services"
description = "Every AWS service fakecloud implements, with operation counts and notable features."
sort_by = "weight"
weight = 3
template = "docs.html"
page_template = "docs-page.html"
+++

fakecloud implements 22 AWS services with 1,668 total operations, all at 100% Smithy conformance. Per-service feature matrices and gotchas live on individual service pages — use the sidebar to navigate.

| Service                | Ops | Notes                                                                  |
| ---------------------- | --- | ---------------------------------------------------------------------- |
| S3                     | 107 | Versioning, lifecycle, notifications, multipart, replication, website  |
| SQS                    |  23 | FIFO, DLQs, long polling, batch                                        |
| SNS                    |  42 | Fan-out to SQS/Lambda/HTTP, filter policies                            |
| EventBridge            |  57 | Pattern matching, schedules, archives, replay, API destinations        |
| Lambda                 |  85 | Real code execution in Docker, 13 runtimes, event source mappings      |
| DynamoDB               |  57 | Transactions, PartiQL, backups, global tables, streams                 |
| IAM                    | 176 | Users, roles, policies, groups, instance profiles, OIDC/SAML           |
| STS                    |  11 | AssumeRole, session tokens, federation                                 |
| SSM                    | 146 | Parameters, documents, commands, maintenance, patch baselines          |
| Secrets Manager        |  23 | Versioning, rotation via Lambda, replication                           |
| CloudWatch Logs        | 113 | Groups, streams, subscription filters, query language                  |
| KMS                    |  53 | Encryption, aliases, grants, real ECDH, key import                     |
| CloudFormation         |  90 | Template parsing, resource provisioning, custom resources              |
| SES (v2 + v1 inbound)  | 110 | Sending, templates, DKIM, real receipt rule execution                  |
| Cognito User Pools     | 122 | Pools, clients, MFA, identity providers, full auth flows               |
| Kinesis                |  39 | Streams, records, shard iterators, retention                           |
| RDS                    | 163 | Real Postgres, MySQL, MariaDB via Docker                               |
| ElastiCache            |  75 | Real Redis, Valkey via Docker                                          |
| Step Functions         |  37 | Full ASL interpreter, Lambda/SQS/SNS/EventBridge/DynamoDB tasks        |
| API Gateway v2         |  28 | HTTP APIs, Lambda proxy, JWT/Lambda authorizers, CORS                  |
| Bedrock                | 101 | Foundation models, guardrails, custom models, invocation/eval jobs    |
| Bedrock Runtime        |  10 | InvokeModel, Converse, streaming, configurable responses, fault inject |

Detailed per-service pages are coming. If you need specifics on a service today, the conformance baseline at [`conformance-baseline.json`](https://github.com/faiscadev/fakecloud/blob/main/conformance-baseline.json) lists every operation fakecloud handles, and the AWS Smithy models in [`aws-models/`](https://github.com/faiscadev/fakecloud/tree/main/aws-models) are the authoritative source of truth.
