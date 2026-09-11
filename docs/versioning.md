# Versioning and deprecation policy

## What is versioned

| Artifact | Version field | Compatibility rule |
|---|---|---|
| Wire schema | `schema_version` (envelope, error responses) | additive changes only within a major version; new fields are optional |
| Module contract | module `version` in descriptors | semantic version; function removal is breaking |
| Function | function `version` in descriptors | semantic version; changing output shape or domain is breaking |
| Numerical policy | `NUMERICAL_POLICY_VERSION` | bumped when rounding defaults, promotion rules, quantile conventions, or standardization definitions change |
| Engine | crate version and `engine.version` in envelopes | semantic version; module versions recorded per result |

A change to a rounding default, quantile convention, promotion rule, or
standardization definition can alter downstream financial or statistical
results even when Rust types are unchanged. Such a change is an explicit
compatibility event: bump `NUMERICAL_POLICY_VERSION`, document it in
`CHANGELOG.md`, and note it in the affected function's version.

## Deprecation

- Deprecated functions stay registered and callable, carry a `deprecated`
  descriptor with `since`, `message`, and an optional `replacement`, and are
  surfaced in `list_functions` and `describe_function`.
- A deprecated function is removed only in a major release and only after at
  least one minor release of deprecation notice.
- Behaviour changes to an existing function are never silent: the function
  version changes and the changelog records the old and new behaviour.

## Fingerprints

Fingerprints include the wire schema version and numerical policy version.
Consequently, a policy change produces different fingerprints for the same
request, which is intentional: it signals that a result may differ and that a
stored receipt may not replay identically.

## MSRV

The minimum supported Rust version is 1.88 and is declared in the workspace
manifest. Raising the MSRV is treated as a minor change and recorded in the
changelog.

## Publication

Preparing release artifacts is in scope. Publishing packages, creating a public
repository, or deploying a service requires the repository owner's explicit
publication instruction and is never performed automatically.
