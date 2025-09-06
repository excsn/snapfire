# SnapFire Project

![Project Status: Active](https://img.shields.io/badge/status-active-success.svg)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](rs/LICENSE)

## About SnapFire

SnapFire is a project focused on creating ergonomic, developer-friendly web templating engines with integrated live-reload functionality.

The core goal is to provide a seamless and productive "hot reload" development experience, where changes to templates and static assets are instantly reflected in the browser. This is combined with a focus on production-readiness, ensuring that all development-only features are completely removed from final builds for maximum performance.

## Implementations

This repository may contain rust implementation of the SnapFire project.

### **Rust Implementation**

The primary and most complete implementation is written in Rust. It features a deep integration with the **Tera** templating engine and the **Actix Web** framework.

**➡️ For detailed information, usage instructions, and the source code, please see the Rust project directory: [`/rs`](./rs)**

## Core Principles

-   **High Developer Experience:** The primary goal is to make the development feedback loop as fast and seamless as possible.
-   **Zero-Overhead Abstractions:** Development-only features must not impact the performance or binary size of production builds.
-   **Ergonomic API:** Library usage should feel simple, intuitive, and natural within the target language's ecosystem.
-   **Configurability:** Provide sensible defaults that work out-of-the-box, but allow power-users to override settings for custom setups.

## License

This project and its implementations are licensed under the **Mozilla Public License 2.0**. See the `LICENSE` file within each implementation's directory for the full text.