# ADR-0021: Path traversal mitigation

## Context

Security review found that `RelPath` values received from network peers were
joined onto anchor roots without validation. A malicious peer could send
`../../../etc/passwd` as a relative path to read or write files outside the
configured sync directory. Affected code: `get_files_validate`,
`put_file_validate`, `mkdir_remote`, `delete_remote`, `rename_remote`, and
`handle_quick_send` in `crates/syncnet/src/handler.rs`.

## Decision

1. Added `RelPath::is_safe()` which rejects paths starting with `/`, containing
   `../`, ending with `..`, or equal to `..`.
2. Added `RelPath::safe_resolve(root)` which joins the path and verifies the
   canonical result stays within the root (defense in depth).
3. All handler methods now call `validate_rel_path()` or inline `is_safe()`
   checks before any filesystem access.
4. `handle_quick_send` additionally validates `destination_dir` doesn't
   contain `..` components.
5. The `rename_remote` method also validates the `new_name` parameter.

## Consequences

- A malicious peer sending traversal paths gets `AccessDenied` errors.
- Legitimate relative paths (no `..` components) are unaffected.
- The pairing auto-accept (`|_fp| true`) remains as a known limitation for
  M7 — mitigated by requiring LAN access and post-hoc fingerprint verification.
