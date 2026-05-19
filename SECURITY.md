# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in `subms`, please **do not** open a
public issue. Use GitHub's private vulnerability reporting:

> Repository -> Security -> Report a vulnerability

Or email the maintainers privately at `security@submillisecond.com`. Provide:

- a description of the issue
- steps to reproduce
- the crate version (Rust) or artifact version (Java) you observed it on
- any proof-of-concept code or output

We aim to acknowledge reports within **5 business days** and to publish a fix
or mitigation within **30 days** of acknowledgement, depending on severity.

## Supported versions

The latest tagged minor release receives security fixes. Older tags are
maintained on a best-effort basis only. Once `v1.0.0` ships, the previous
major will receive security fixes for 6 months.

## Out of scope

- Findings in user-supplied bench commands. The harness measures whatever
  you tell it to measure; it does not validate the workload's safety.
- Findings against third-party sinks (Slack, Datadog, AWS S3, etc.) that the
  separate `subms-action-*` repos forward data to - those are the
  responsibility of the respective vendors.
- Performance numbers being lower than expected on a given hardware tier.
  The library reports what it measures; it does not warrant performance.
