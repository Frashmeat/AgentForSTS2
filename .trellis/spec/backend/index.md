# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

This directory contains guidelines for backend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Rust workspace ownership, Platform DDD, Game Pack and script placement | Active |
| [Database Guidelines](./database-guidelines.md) | Explicit no-database desktop boundary and future cutover gate | Active / N/A |
| [Error Handling](./error-handling.md) | ActionableFailure, cancellation, IPC/React mapping and redaction | Active |
| [Quality Guidelines](./quality-guidelines.md) | Run/Artifact, generation, Game Pack, Truth Snapshot, ProjectSession and candidate provenance contracts | Active |
| [Logging Guidelines](./logging-guidelines.md) | tracing/desktop diagnostics, redaction and release-output boundary | Active |
| [Release Guidelines](./release-guidelines.md) | Windows candidate CLI, verification schema, manifests and CI boundary | Active |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

---

**Language**: All documentation should be written in **English**.
