# Security Policy

## Supported Versions

`0.1.0` is a Beta release line. Security fixes should target the current
public Beta branch first.

## Reporting A Vulnerability

Do not file public issues for vulnerabilities. Use GitHub private vulnerability
reporting if it is enabled for the repository; otherwise contact the
maintainers through the private channel used for this project.

Useful reports include:

- affected commit or release;
- operating system version;
- reproduction steps;
- whether the issue involves the local control socket, terminal command
  execution, workspace paths, browser navigation, or persisted state;
- any logs or screenshots that do not contain secrets.

## Current Security Boundary

AgentHouse currently exposes a local Unix-domain JSONL control socket for
automation and smoke tests. It is local-only and is not a network API. Do not
expose it over the network or treat it as authenticated multi-user
infrastructure.
