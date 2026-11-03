# group-host

The opensocial group host, in Rust: the PDS, space authority and OAuth
server for groups on atproto, and the console where people start and run
them. It is opensocial's `packages/group-host` and its console, written
again as knotless is built, and wire-compatible with it.

```sh
just setup
just serve      # the host, on :2600
just web-dev    # the console
```

[docs/design](docs/design/README.md) says how it is put together and what
is still to come over; [docs/development.md](docs/development.md) how to
build and test it. AGENTS.md is for agents working here.

Licensed under MIT or Apache-2.0, at your option.
