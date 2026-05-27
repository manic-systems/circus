# Design

Notes to self and somewhat of a guide to design some of the design choices
behind Circus. This is not a contribution guideline, and changes to this document
are welcome if necessary.

## Overview

Circus is built as a local replacement for Hydra. Meaning you probably do not want
to deploy it on your Super Enterprise Friends Group that needs a reliable CI.
This project is an attempt to utilize our infrastructure members as build
machines to cache our projects without relying on Github's weak runners.

---

Hydra is the Nix/NixOS project's continuous integration system. It uses Nix to
declaratively define and build jobs, ensuring reproducible builds. According to
the NixOS Wiki:

> "Hydra is a tool for continuous integration testing and software release that
> uses a purely functional language to describe build jobs and their
> dependencies."

In Hydra:

- A **Project** corresponds to a source repository.
- A **Jobset** (often per branch or channel) contains many **Jobs** (Nix
  derivations to build).
- A `release.nix` ("Release Set") file declares what to build.

Hydra pulls changes from version control, re-evaluates Nix expressions, and
triggers builds when inputs change.

Circus _more or less_ commits to this design, with minimal tweaks for modernity and
UX. Most critically, Circus is not designed to be used alongside Nixpkgs. Indeed,
you _could_ do it and I am more than willing to try and support this use case
but it is far from the main goal. The primary purpose of Circus is to be a
distributed, declarative CI that **has learned from Hydra's mistakes**.

## On Hydra

### Component Interactions and Data Flow

Hydra follows a tightly-coupled architecture with three main daemons:

```plaintext
Git Repository -> Evaluator -> Database -> Queue Runner -> Build Hosts -> Results -> Database - Web UI
```

1. **hydra-server** (Perl/Catalyst): Web interface and REST API
2. **hydra-evaluator**: Polls Git repos, evaluates Nix expressions, creates
   `.drv` files
3. **hydra-queue-runner**: Dispatches builds to available builders via SSH/Nix
   remote
4. **Database (PostgreSQL)**: Central state management for all components
