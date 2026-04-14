+++
title = "SNS"
description = "Topics, subscriptions, fan-out delivery, filter policies, platform applications."
weight = 3
+++

fakecloud implements **42 of 42** SNS operations at 100% Smithy conformance.

## Supported features

- **Topics** — CRUD, attributes, tags, policies
- **Subscriptions** — CRUD with confirmation flow, filter policies, raw message delivery
- **Fan-out delivery** — SQS, Lambda, HTTP/HTTPS subscriptions actually deliver
- **Filter policies** — JSON filter matching on message attributes and body
- **Platform applications** — iOS/Android/FCM/APNS endpoints (recorded for introspection, not sent)
- **Message deduplication** — via attribute
- **FIFO topics** — ordering, group IDs

## Protocol

Query protocol. Form-encoded body, `Action` parameter, XML responses.

## Introspection

- `GET /_fakecloud/sns/messages` — list all published messages
- `GET /_fakecloud/sns/pending-confirmations` — list subscriptions pending confirmation
- `POST /_fakecloud/sns/confirm-subscription` — force-confirm an SNS subscription

## Cross-service delivery

- **SNS → SQS / Lambda / HTTP** — Fan-out delivery to all subscription types

## Gotchas

- **Email and SMS delivery are not real.** Messages to email or SMS endpoints are recorded for introspection at `/_fakecloud/sns/messages` but never actually sent. There's no SMTP or SMS gateway.
- If you need real email testing, use SES instead.

## Source

- [`crates/fakecloud-sns`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-sns)
- [AWS SNS API reference](https://docs.aws.amazon.com/sns/latest/api/Welcome.html)
