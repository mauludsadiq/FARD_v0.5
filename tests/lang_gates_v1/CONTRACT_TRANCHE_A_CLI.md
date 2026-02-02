# Tranche A: Package + export + lock generation — CLI contracts (normative)

This document pins the required CLI surface for Tranche A. Gates rely on these exact flags/outputs.

## 1) Package manifest

File: `fard.pkg.json` at package root

Required keys:
- schema: "fard.pkg.v0_1"
- name: string
- version: semver string
- exports: object mapping module_name -> relative_path
Optional:
- deps: object mapping dep_name -> semver string

## 2) Export surface (language)

In a module file:
- `export { name1, name2, ... }` declares the only visible bindings to importers.
- Accessing a non-exported binding via an imported module MUST fail deterministically with a structured error:
  - stderr contains tag: EXPORT_MISSING
  - error.json exists with code: ERROR_EXPORT_MISSING (or ERROR_RUNTIME with message containing EXPORT_MISSING)

## 3) Lock generation: fardlock

Binary: `fardlock`

Command:
- `fardlock gen --root <pkg_root> --registry <registry_dir> --out <out_dir>`

Outputs in `<out_dir>`:
- `fard.lock.json` with schema "fard.lock.v0_1"
- `fard.lock.json.cid` (sha256:<hex> newline)

Lock file structure (minimum):
- schema
- package: { name, version }
- deps: object keyed by dep name:
  - version
  - digest (sha256:<hex>) for the resolved package bundle digest
  - exports: object (module_name -> digest) or a digest for export table

Determinism:
- Running `fardlock gen` twice over identical inputs MUST produce byte-identical `fard.lock.json`.

## 4) Package publish to registry: fardpkg

Binary: `fardpkg`

Commands:
- `fardpkg publish --root <pkg_root> --registry <registry_dir> --out <out_dir>`

Outputs in `<out_dir>`:
- `publish.json` with schema "fard.publish.v0_1"
- includes:
  - name, version
  - bundle_digest (sha256:<hex>)
  - registry_path (relative path written)

Registry write:
- writes a content-addressed package bundle under:
  - `<registry_dir>/<name>/<version>/bundle/`
  - includes bundle.json, imports.lock.json, files/*
- MUST be deterministic for same package bytes + toolchain.

## 5) Running an app with packages: fardrun

Binary: `fardrun`

Command:
- `fardrun run <entry_file> --out <out_dir> --lock <lockfile> --registry <registry_dir>`

Resolution rule:
- imports with prefix `pkg:<name>@<version>/...` resolve against `<registry_dir>/<name>/<version>/bundle/`
- loader MUST enforce that:
  - lockfile dependency digests match registry package bundle digest (LOCK_MISMATCH on failure)
  - export table is enforced (EXPORT_MISSING on failure)
