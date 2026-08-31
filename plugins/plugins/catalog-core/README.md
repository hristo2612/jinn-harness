# `catalog-core` — the shared half of a catalog provider

Not a crate. Shared SOURCE, included by both providers with
`#[path = "../../catalog-core/catalog.rs"] mod catalog;`, for the reason
the other seams' `store-core` is — a guest generates its OWN
`wit_bindgen::generate!` bindings, so a library crate cannot make host
calls on the guest's behalf. Its one home is
`plugins/sessions/store-core/README.md`.

Everything that is not a host call already lives in `jinn-plugins`: the
reading law, the transition table, the attribution filter, the grant
source and every answer's shape. What is left is the three reads and
the order they happen in, and it is identical in both providers.

## What an including crate supplies

- `PROVIDER: &str` — the package name every answer reports as
  `served-by`. This is what makes a provider SWAP observable in the
  answer itself rather than only in the profile.
- `SOURCE: GrantSource` — whether its entry set and grant lists are the
  document of record or its own declaration.
- `mod source` with
  `fn declared(config: &CatalogConfig) -> Result<Vec<Declared>, PluginsError>`.

## The read order

`declared` → `jinn:introspect entries` → `jinn:ledger last-seq` +
`read-range`. Three reads at three instants; the window and the
`JOIN_QUALIFIER` that says so ride on every answer.

A read that refuses is a typed `unavailable` naming the contract. It is
never an empty catalog, an absent grant list, or a lifecycle: the
difference between "I read it and there is nothing" and "I could not
read it" is the whole point of the seam.
